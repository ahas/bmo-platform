"""Carries a run's long tools from its header into the request a backend reads.

A run names the tools of its program whose calls wait long in one header, which the program sends on
every request of the run; this hook writes the header's value, unchanged, into one field of the
request body before it is routed, where an engine of the node pool reads it to know that a call
the answer ends on waits long (docs/own-llm.md, 「The gateway」).

The field is a string, the header's value as it came: the names, joined by commas. The hook reads no
name and keeps no list of them; the names are the program's own. A request with no such header, or
an empty one, gets no field, and the engines take the field out before they pass a request on.
"""

from litellm.integrations.custom_logger import CustomLogger

# The header a run names its long tools in, as the program's custom headers send it.
LONG_TOOLS_HEADER = "x-bmo-long-tools"
# The body field the engines read them from.
LONG_TOOLS_FIELD = "bmo_long_tools"


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


def carry_long_tools(data: dict) -> dict:
    """The request body with its header's long tools written into the field the engines read."""
    named = header_value(data, LONG_TOOLS_HEADER)
    if isinstance(named, str) and named != "":
        data[LONG_TOOLS_FIELD] = named
    return data


class LongToolCarrier(CustomLogger):
    async def async_pre_call_hook(self, user_api_key_dict, cache, data, call_type):
        return carry_long_tools(data)


handler = LongToolCarrier()
