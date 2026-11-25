"""Takes the wait off every request.

No request on the gateway is given up on a clock: it is waited for until its backend answers or its
connection is refused or broken (docs/own-llm.md, 「The gateway」). LiteLLM has no value that means no wait —
an entry naming none falls to the proxy-wide default, and an infinite one fails the connection — so no entry
names one, and after the entry a request goes to is chosen and before the request is sent, this hook hands
the request a wait with no limit.
"""

import httpx

from litellm.integrations.custom_logger import CustomLogger


def take_the_wait_off(kwargs: dict):
    """The request with no limit on its wait."""
    kwargs["timeout"] = httpx.Timeout(None)
    kwargs.pop("stream_timeout", None)
    return kwargs


class Unclocked(CustomLogger):
    async def async_pre_call_deployment_hook(self, kwargs, call_type):
        return take_the_wait_off(kwargs)


handler = Unclocked()
