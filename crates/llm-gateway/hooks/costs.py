"""Hands the front what each request cost.

Every request that ends, answered or failed, is told to the front at `POST /gateway/costs`, under the child's
own key, at the address the launcher hands the child as `LLM_GATEWAY_FRONT`: LiteLLM's cost of it in US
dollars — the provider's own word where its answer's usage names a cost, else LiteLLM's — the model the caller named, its input and output tokens, the payer the front named it in
`x-bmo-payer`, the run its `x-bmo-run` header names and the program's session its Claude Code session header
names. The front writes one request record of it (docs/own-llm.md, 「What a request costs」).

Nothing here may fail a request: a cost that cannot be told is said in one line and dropped.
"""

import os

import httpx

from litellm.integrations.custom_logger import CustomLogger

# The headers a request is told by: its payer, as the front names it, its run and its program's session.
PAYER_HEADER = "x-bmo-payer"
RUN_HEADER = "x-bmo-run"
SESSION_HEADER = "x-claude-code-session-id"
# Where the front takes a cost, under its own address.
COSTS_PATH = "/gateway/costs"


def request_headers(kwargs: dict) -> dict:
    """The headers the caller's request came with, as the proxy kept them."""
    params = kwargs.get("litellm_params")
    request = params.get("proxy_server_request") if isinstance(params, dict) else None
    if not isinstance(request, dict):
        request = kwargs.get("proxy_server_request")
    headers = request.get("headers") if isinstance(request, dict) else None
    return headers if isinstance(headers, dict) else {}


def header_value(headers: dict, name: str):
    """The header of that name, whatever its case, as a string, or None."""
    for key, value in headers.items():
        if isinstance(key, str) and key.lower() == name and isinstance(value, str) and value.strip():
            return value.strip()
    return None


def number(value):
    """A number as it stands, or None where it is none."""
    return value if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def whole(value):
    named = number(value)
    return int(named) if named is not None else None


def usage_read(response_obj):
    """A reader of the usage an answer names, or None where it names none."""
    usage = getattr(response_obj, "usage", None)
    if usage is None and isinstance(response_obj, dict):
        usage = response_obj.get("usage")
    if usage is None:
        return None
    return usage.get if isinstance(usage, dict) else lambda name: getattr(usage, name, None)


def usage_of(response_obj):
    """The prompt and completion tokens an answer names, where it names them."""
    read = usage_read(response_obj)
    if read is None:
        return None, None
    return whole(read("prompt_tokens")), whole(read("completion_tokens"))


def provider_cost(response_obj):
    """What the provider itself says the request cost, in its answer's usage, as OpenRouter does; None where
    it says nothing. The provider's own word comes first, since LiteLLM's price map holds no price for many of
    the models a provider serves."""
    read = usage_read(response_obj)
    return number(read("cost")) if read is not None else None


def cost_of(kwargs: dict, response_obj) -> dict:
    """What the front is told of one request."""
    logged = kwargs.get("standard_logging_object")
    logged = logged if isinstance(logged, dict) else {}
    headers = request_headers(kwargs)
    cost = provider_cost(response_obj)
    if cost is None:
        cost = number(kwargs.get("response_cost"))
    if cost is None:
        cost = number(logged.get("response_cost"))
    model = logged.get("model_group") or kwargs.get("model") or logged.get("model")
    input_tokens, output_tokens = whole(logged.get("prompt_tokens")), whole(logged.get("completion_tokens"))
    if input_tokens is None and output_tokens is None:
        input_tokens, output_tokens = usage_of(response_obj)
    return {
        "payer": header_value(headers, PAYER_HEADER),
        "run": header_value(headers, RUN_HEADER),
        "session": header_value(headers, SESSION_HEADER),
        "model": model if isinstance(model, str) else None,
        "costUsd": cost if cost is not None else 0.0,
        "inputTokens": input_tokens,
        "outputTokens": output_tokens,
    }


async def tell(kwargs: dict, response_obj):
    """The request's cost posted to the front; a failure is said in one line and goes no further."""
    try:
        front = os.environ["LLM_GATEWAY_FRONT"]
        key = os.environ["LLM_GATEWAY_KEY"]
        async with httpx.AsyncClient() as client:
            answer = await client.post(
                front.rstrip("/") + COSTS_PATH,
                json=cost_of(kwargs, response_obj),
                headers={"authorization": f"Bearer {key}"},
            )
        if answer.status_code >= 300:
            print(f"costs: the front answered {answer.status_code} to a request's cost", flush=True)
    except Exception as fault:  # noqa: BLE001 - nothing of the hook's may reach the request
        print(f"costs: a request's cost could not be told to the front: {type(fault).__name__}: {fault}", flush=True)


class CostTeller(CustomLogger):
    async def async_log_success_event(self, kwargs, response_obj, start_time, end_time):
        await tell(kwargs, response_obj)

    async def async_log_failure_event(self, kwargs, response_obj, start_time, end_time):
        await tell(kwargs, response_obj)


handler = CostTeller()
