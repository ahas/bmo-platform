"""Carries a Messages request's thinking to a backend that reads it from its chat template.

The caller decides whether the model thinks, and on Anthropic's Messages API it says so in
`thinking`. The backends behind this gateway are OpenAI-compatible chat servers that read the
choice as `enable_thinking` in `chat_template_kwargs`, and the gateway's own translation of
`thinking` neither reaches that nor stays on the chat path: it becomes OpenAI's reasoning effort,
which the template does not read, and sends the request to the Responses API, which the backends
do not serve. So the choice is written where the template reads it and the Anthropic form of it is
taken out before the request is routed, and the request goes to chat completions like any other
(docs/own-llm.md, 「The gateway」).

The answer needs nothing from here: the gateway already turns the `reasoning_content` a backend
answers with into the Messages answer's thinking block, streamed or not.
"""

from litellm.integrations.custom_logger import CustomLogger

# The `thinking` types that ask the model to think, and the one that asks it not to. Any other
# value names nothing, and the table's default stands.
THINKING_ON = frozenset({"enabled", "adaptive"})
THINKING_OFF = "disabled"


def carry_thinking(data: dict) -> dict:
    """The Messages request body with its thinking choice moved into `chat_template_kwargs`.

    A request that names `enable_thinking` itself is asked as it named it; the Anthropic fields are
    taken out all the same, so that nothing sends it to the Responses API.
    """
    thinking = data.pop("thinking", None)
    output_config = data.get("output_config")

    # The effort is Anthropic's depth of thinking, which a chat template has no word for. Left in, it
    # becomes a reasoning effort the backend does not read; on its own it never turns thinking on.
    if isinstance(output_config, dict) and "effort" in output_config:
        rest = {key: value for key, value in output_config.items() if key != "effort"}
        if rest:
            data["output_config"] = rest
        else:
            del data["output_config"]

    kind = thinking.get("type") if isinstance(thinking, dict) else None
    if kind in THINKING_ON:
        enable = True
    elif kind == THINKING_OFF:
        enable = False
    else:
        return data

    template = data.get("chat_template_kwargs")
    template = dict(template) if isinstance(template, dict) else {}
    template.setdefault("enable_thinking", enable)
    data["chat_template_kwargs"] = template
    return data


class ThinkingCarrier(CustomLogger):
    async def async_pre_call_hook(self, user_api_key_dict, cache, data, call_type):
        if call_type != "anthropic_messages":
            return data
        return carry_thinking(data)


handler = ThinkingCarrier()
