"""Starts a request from saved state on an instance that keeps role files: its session's, else its role's.

An engine instance that saves and restores its conversation state keeps one file per role, the state of
that role's system prompt read once, named `role-<role>.bin`; restoring it costs a fraction of a second
where reading the prompt again costs many (docs/own-llm.md, 「Role files」). A run whose work key names a
role says so in a header on every request. After the instance a request goes to is chosen and before the
request is sent, this hook asks that instance to restore the role's file where the instance is not on that
role already, then lets the request go on. Which files stand, and whether one is still valid, are the
engine deployment's own; this hook never saves one.

For each instance the hook keeps the role it last restored or served: a request naming no role leaves the
instance on none, since what it reads replaces the role's state. A restore is waited for until the instance
answers it; one that is refused, or whose connection is refused or broken, leaves the request to go as it
stands, with a line in the log, and that role is not asked of that instance
again for ten minutes. What it keeps is this process's: a new gateway child starts with every instance on
no role.

Such an instance holds one conversation at a time, so a request of another session takes the place of the
one it held, and that one's next request would be read again from its first token. The hook keeps for each
instance the session it last served, as the program's session header names it. Before a request of another
session, or of none, is sent, it has the instance save the session it holds into one of a few files named
`session-<n>.bin`, where what that session last read is long enough to be worth a file; and a request whose
session has a file on that instance has it restored first, in place of the role's file, since the session's
state stands past the role's prefix (docs/own-llm.md, 「Session files」). The files of an instance are a fixed
few, the one least recently used written over, so they take a bounded space on the node.
"""

import asyncio
import collections
import json
import re
import sys
import time
import urllib.error
import urllib.request

from litellm.integrations.custom_logger import CustomLogger

# The header a run names its role in, as the program's custom headers send it.
ROLE_HEADER = "x-bmo-role"
# What a role's name looks like; anything else is no role.
ROLE_NAME = re.compile(r"^[a-z][a-z0-9-]{0,39}$")
# How long a role an instance could not restore is not asked of it again, in seconds.
RETRY_AFTER = 600

# The header a program names its session in, on every request it makes.
SESSION_HEADER = "x-claude-code-session-id"
# What a session's name looks like; anything else is no session.
SESSION_NAME = re.compile(r"^[A-Za-z0-9_-]{1,80}$")
# How many session files one instance keeps; a further session takes the file least recently used.
SESSION_FILES = 3
# The tokens a session last read from which leaving it saves it: below this the instance's own parked
# conversations in memory bring it back, and a file would cost more than it saves.
SAVE_FROM_TOKENS = 32_000

# Each instance's session, by its root address: the one it last served, None for none or not known.
on_session: dict = {}
# Each instance's session files, by its root address: the file number of each session that has one, the
# least recently used first.
session_files: dict = {}
# Each instance's role, by its root address: the one it last restored or served, None for none.
on_role: dict = {}
# When each role an instance could not restore may be asked of it again, by (root, role).
refused_until: dict = {}
# One check and restore at a time for each instance, so two requests do not restore it twice.
locks: dict = {}


def log(text: str) -> None:
    print(f"llm-gateway roles: {text}", file=sys.stderr, flush=True)


def headers_of(kwargs: dict) -> dict:
    """The request's headers, wherever the router left them beside the deployment's call."""
    places = [
        kwargs.get("proxy_server_request"),
        (kwargs.get("litellm_params") or {}).get("proxy_server_request"),
    ]
    for request in places:
        if isinstance(request, dict) and isinstance(request.get("headers"), dict):
            return request["headers"]
    for name in ("metadata", "litellm_metadata"):
        metadata = kwargs.get(name)
        if isinstance(metadata, dict) and isinstance(metadata.get("headers"), dict):
            return metadata["headers"]
    return {}


def named(kwargs: dict, header: str, shape):
    """What the request names in the header, or None where it names nothing of that shape."""
    for key, value in headers_of(kwargs).items():
        if isinstance(key, str) and key.lower() == header and isinstance(value, str):
            name = value.strip()
            return name if shape.match(name) else None
    return None


def role_of(kwargs: dict):
    """The role the request names, or None."""
    return named(kwargs, ROLE_HEADER, ROLE_NAME)


def session_of(kwargs: dict):
    """The session the request names, or None."""
    return named(kwargs, SESSION_HEADER, SESSION_NAME)


def keeps_role_files(kwargs: dict) -> bool:
    info = kwargs.get("model_info")
    return isinstance(info, dict) and info.get("role_files") is True


def instance_root(api_base) -> str:
    """The instance's own address: its chat address without the path the OpenAI client asks under."""
    if not isinstance(api_base, str):
        return ""
    root = api_base.rstrip("/")
    for tail in ("/chat/completions", "/v1"):
        if root.endswith(tail):
            root = root[: -len(tail)]
    return root


def ask(root: str, key, path: str, body=None):
    """Whether the instance answered, with its answer read as JSON, or why not where it did not."""
    headers = {"Content-Type": "application/json"}
    if isinstance(key, str) and key and key != "no-key":
        headers["Authorization"] = f"Bearer {key}"
    data = None if body is None else json.dumps(body).encode()
    asked = urllib.request.Request(f"{root}{path}", data=data, method="GET" if body is None else "POST", headers=headers)
    try:
        with urllib.request.urlopen(asked) as answer:
            text = answer.read()
    except urllib.error.HTTPError as refusal:
        return False, f"the instance answered {refusal.code}"
    except Exception as failure:  # a refused or broken connection
        return False, f"no answer: {failure}"
    try:
        return True, json.loads(text)
    except ValueError:
        return True, None


def slot(root: str, key, action: str, file: str):
    """Whether the instance saved its conversation into the file, or restored it from it, and why not."""
    done, answer = ask(root, key, f"/slots/0?action={action}", {"filename": file})
    return done, None if done else answer


def last_read(root: str, key) -> int:
    """The tokens of the prompt the instance last read, as its own figures say; none where they do not."""
    done, figures = ask(root, key, "/metrics")
    cache = figures.get("conversation_cache") if done and isinstance(figures, dict) else None
    tokens = cache.get("last_prompt") if isinstance(cache, dict) else None
    return tokens if isinstance(tokens, int) else 0


def file_of(root: str, session: str) -> str:
    """The session's file on the instance: its own, else a free one, else the one least recently used."""
    files = session_files.setdefault(root, collections.OrderedDict())
    if session in files:
        number = files.pop(session)
    elif len(files) < SESSION_FILES:
        number = min(set(range(SESSION_FILES)) - set(files.values()))
    else:
        _, number = files.popitem(last=False)
    files[session] = number
    return f"session-{number}.bin"


async def leave_session(root: str, key, session: str) -> None:
    """The session the instance holds saved into its file where it is long enough, and left without one otherwise."""
    files = session_files.setdefault(root, collections.OrderedDict())
    if await asyncio.to_thread(last_read, root, key) < SAVE_FROM_TOKENS:
        files.pop(session, None)
        return
    file = file_of(root, session)
    began = time.monotonic()
    saved, why = await asyncio.to_thread(slot, root, key, "save", file)
    if saved:
        log(f"{root} saved the session it held into {file} in {time.monotonic() - began:.2f} s")
    else:
        files.pop(session, None)
        log(f"{root} did not save the session it held into {file} ({why}); its next request is read from the start")


async def return_to_session(root: str, key, session: str) -> bool:
    """Whether the instance restored the session's file, where the session has one."""
    files = session_files.setdefault(root, collections.OrderedDict())
    if session not in files:
        return False
    file = file_of(root, session)
    began = time.monotonic()
    restored, why = await asyncio.to_thread(slot, root, key, "restore", file)
    if restored:
        log(f"{root} restored {file} in {time.monotonic() - began:.2f} s")
    else:
        files.pop(session, None)
        log(f"{root} did not restore {file} ({why}); the request goes as it stands")
    return restored


async def start_from_session(root: str, key, session) -> bool:
    """The instance put on the request's session; whether the session's own file was restored for it."""
    held = on_session.get(root)
    if held == session:
        return False
    if held is not None:
        await leave_session(root, key, held)
    on_session[root] = session
    return session is not None and await return_to_session(root, key, session)


async def start_from_role(kwargs: dict) -> None:
    """The instance the request goes to put on the request's session or role first, where it keeps role files."""
    if not keeps_role_files(kwargs):
        return
    root = instance_root(kwargs.get("api_base"))
    if not root:
        return
    role = role_of(kwargs)
    key = kwargs.get("api_key")
    lock = locks.setdefault(root, asyncio.Lock())
    async with lock:
        if await start_from_session(root, key, session_of(kwargs)):
            on_role[root] = role
            return
        if role is None:
            on_role[root] = None
            return
        if on_role.get(root) == role:
            return
        if refused_until.get((root, role), 0) > time.monotonic():
            on_role[root] = None
            return
        began = time.monotonic()
        restored, why = await asyncio.to_thread(slot, root, key, "restore", f"role-{role}.bin")
        took = time.monotonic() - began
        if restored:
            on_role[root] = role
            log(f"{root} restored role-{role}.bin in {took:.2f} s")
        else:
            on_role[root] = None
            refused_until[(root, role)] = time.monotonic() + RETRY_AFTER
            log(f"{root} did not restore role-{role}.bin ({why}); the request goes as it stands, and the role is not asked of it again for {RETRY_AFTER // 60} minutes")


class RoleStarter(CustomLogger):
    async def async_pre_call_deployment_hook(self, kwargs, call_type):
        await start_from_role(kwargs)
        return None


handler = RoleStarter()
