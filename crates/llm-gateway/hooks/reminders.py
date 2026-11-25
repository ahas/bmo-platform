"""Folds a system turn inside a Messages conversation into the person's turn beside it.

Claude Code sends some of what it tells a run as system-role messages among the conversation's own,
its environment block right after a session's first words among them. The backends behind this
gateway read a system message only at the head of their chat template, so the gateway turns each
one into a user message of its own; standing after the person's words, it is then the last thing
the model reads, and a model may answer by repeating it. This hook puts each such run inside the
user message it follows, ahead of the person's words, or, where none comes before it, inside the
one after it (docs/own-llm.md, 「The gateway」).

Only the request's own order is read, so the same conversation sent again on the next request is
folded the same way and a backend's saved state for it still matches.
"""

from litellm.integrations.custom_logger import CustomLogger

# The tags a reminder stands between inside a user turn, as Claude Code itself writes its own.
REMINDER_OPEN = "<system-reminder>"
REMINDER_CLOSE = "</system-reminder>"


def is_system(message) -> bool:
    return isinstance(message, dict) and message.get("role") == "system"


def is_user(message) -> bool:
    return isinstance(message, dict) and message.get("role") == "user"


def blocks_of(content) -> list:
    """A message's content as a list of blocks, a bare string being one text block."""
    if isinstance(content, str):
        return [{"type": "text", "text": content}] if content else []
    if isinstance(content, list):
        return list(content)
    return []


def reminder_blocks(message: dict) -> list:
    """One system message as the blocks it becomes inside a user turn: its text between the tags."""
    blocks = blocks_of(message.get("content"))
    text = "\n".join(
        block["text"]
        for block in blocks
        if isinstance(block, dict) and block.get("type") == "text" and isinstance(block.get("text"), str)
    )
    rest = [block for block in blocks if not (isinstance(block, dict) and block.get("type") == "text")]
    wrapped = [{"type": "text", "text": f"{REMINDER_OPEN}\n{text}\n{REMINDER_CLOSE}"}] if text else []
    return wrapped + rest


def with_reminders(user: dict, run: list) -> dict:
    """The user message with a run of system messages inside it, after the tool results it opens with."""
    blocks = blocks_of(user.get("content"))
    results = 0
    while results < len(blocks) and isinstance(blocks[results], dict) and blocks[results].get("type") == "tool_result":
        results += 1
    reminders = [block for message in run for block in reminder_blocks(message)]
    return {**user, "content": blocks[:results] + reminders + blocks[results:]}


def fold_system_turns(messages: list) -> list:
    """The conversation with every system run that has a user turn beside it folded into that turn."""
    folded: list = []
    index = 0
    while index < len(messages):
        message = messages[index]
        if not is_system(message):
            folded.append(message)
            index += 1
            continue
        end = index
        while end < len(messages) and is_system(messages[end]):
            end += 1
        run = messages[index:end]
        if folded and is_user(folded[-1]):
            folded[-1] = with_reminders(folded[-1], run)
        elif end < len(messages) and is_user(messages[end]):
            folded.append(with_reminders(messages[end], run))
            end += 1
        else:
            folded.extend(run)
        index = end
    return folded


def fold_request(data: dict) -> dict:
    messages = data.get("messages")
    if isinstance(messages, list) and any(is_system(message) for message in messages):
        data["messages"] = fold_system_turns(messages)
    return data


class ReminderFolder(CustomLogger):
    async def async_pre_call_hook(self, user_api_key_dict, cache, data, call_type):
        if call_type != "anthropic_messages":
            return data
        return fold_request(data)


handler = ReminderFolder()
