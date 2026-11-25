"""Carries a run's sampling settings from its header into the request a backend reads.

Claude Code has no word for most of what an OpenAI-compatible chat server samples with — `top_k`,
`min_p`, the repetition and DRY penalties, the seed — and fixes its own `temperature`. A run on a
registration names the settings it is asked with in one header instead, which the program sends on
every request of the run; this hook writes each into the request body before it is routed, over
whatever the program put there, so the backend reads them as a chat request's own
(docs/own-llm.md, 「The gateway」).

The header is a JSON object of names and numbers, with the chat template's own words under
`chat_template_kwargs`, each on, off or a number. A name is the backend's own parameter name;
anything else in it is passed over, and a header that is not such an object changes nothing. Which
settings a run names, and their bounds, are its caller's to decide.

A request whose template words name `preserve_thinking` off has the thinking of every answer before
its last question taken out, so the history it carries holds no more of it than the template renders.
"""

import json
import re

from litellm.integrations.custom_logger import CustomLogger

# The header a run names its sampling settings in, as the program's custom headers send it.
SAMPLING_HEADER = "x-bmo-sampling"
# What a backend parameter name looks like; anything else is not written into the body.
PARAMETER_NAME = re.compile(r"^[a-z][a-z_]{0,39}$")
# Where a chat request carries the words its template reads.
TEMPLATE_FIELD = "chat_template_kwargs"
# The template word that says whether an answer before the last question keeps its thinking.
PRESERVE_THINKING = "preserve_thinking"
# The blocks of a Messages answer that are its thinking.
THINKING_BLOCKS = frozenset({"thinking", "redacted_thinking"})


def header_value(data: dict, name: str):
    """The request's header of that name, whatever its case, or None."""
    request = data.get("proxy_server_request")
    headers = request.get("headers") if isinstance(request, dict) else None
    if not isinstance(headers, dict):
        return None
    for key, value in headers.items():
        if isinstance(key, str) and key.lower() == name:
            return value
    return None


def named_in(text) -> dict:
    """The JSON object a header holds, or nothing."""
    if not isinstance(text, str):
        return {}
    try:
        named = json.loads(text)
    except ValueError:
        return {}
    return named if isinstance(named, dict) else {}


def is_number(value) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and value == value and abs(value) != float("inf")


def sampling_of(named: dict) -> dict:
    """The settings a header names: each backend parameter name with its number."""
    return {key: value for key, value in named.items() if isinstance(key, str) and PARAMETER_NAME.match(key) and is_number(value)}


def template_words_of(named: dict) -> dict:
    """The chat template's words a header names, each on, off or a number."""
    words = named.get(TEMPLATE_FIELD)
    if not isinstance(words, dict):
        return {}
    return {
        key: value
        for key, value in words.items()
        if isinstance(key, str) and PARAMETER_NAME.match(key) and (isinstance(value, bool) or is_number(value))
    }


def entry_template_words(model) -> dict:
    """The template words the table's entry for the model sends where a request names none. A request's
    own words are sent in place of these whole, so words laid over a request's start from them."""
    try:
        from litellm.proxy.proxy_server import llm_router
    except ImportError:
        return {}
    if llm_router is None or not isinstance(model, str):
        return {}
    for entry in llm_router.get_model_list(model_name=model) or []:
        params = entry.get("litellm_params") if isinstance(entry, dict) else None
        words = params.get(TEMPLATE_FIELD) if isinstance(params, dict) else None
        if isinstance(words, dict):
            return dict(words)
    return {}


def is_question(message: dict) -> bool:
    """Whether a message is a question the template renders past thinking up to: a user message carrying
    anything beside tool results, or a system turn, which reaches the template as a user message."""
    role = message.get("role")
    if role == "system":
        return True
    if role != "user":
        return False
    content = message.get("content")
    if not isinstance(content, list):
        return True
    return any(not (isinstance(block, dict) and block.get("type") == "tool_result") for block in content)


def without_thinking(message: dict) -> dict:
    """An answer with its thinking taken out: a Messages answer's thinking blocks, unless they are all it
    holds, and a chat answer's reasoning."""
    kept = {key: value for key, value in message.items() if key not in ("reasoning_content", "thinking_blocks")}
    content = message.get("content")
    if isinstance(content, list):
        words = [block for block in content if not (isinstance(block, dict) and block.get("type") in THINKING_BLOCKS)]
        if words:
            kept["content"] = words
    return kept


def drop_past_thinking(data: dict) -> dict:
    """The request with the thinking of every answer before its last question taken out."""
    messages = data.get("messages")
    if not isinstance(messages, list):
        return data
    last = max((at for at, message in enumerate(messages) if isinstance(message, dict) and is_question(message)), default=-1)
    data["messages"] = [
        without_thinking(message) if at < last and isinstance(message, dict) and message.get("role") == "assistant" else message
        for at, message in enumerate(messages)
    ]
    return data


def carry_sampling(data: dict) -> dict:
    """The request body with every setting its header names written over the body's own, and its past
    thinking taken out where its template words say the template drops it."""
    named = named_in(header_value(data, SAMPLING_HEADER))
    data.update(sampling_of(named))
    words = template_words_of(named)
    if words:
        own = data.get(TEMPLATE_FIELD)
        data[TEMPLATE_FIELD] = {**entry_template_words(data.get("model")), **(own if isinstance(own, dict) else {}), **words}
    template = data.get(TEMPLATE_FIELD)
    if isinstance(template, dict) and template.get(PRESERVE_THINKING) is False:
        drop_past_thinking(data)
    return data


class SamplingCarrier(CustomLogger):
    async def async_pre_call_hook(self, user_api_key_dict, cache, data, call_type):
        return carry_sampling(data)


handler = SamplingCarrier()
