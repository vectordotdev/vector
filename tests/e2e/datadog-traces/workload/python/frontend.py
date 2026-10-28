"""Frontend service: one checkout trace per request, fanning out to the Node and Go services."""

import os
import signal
import sys

import requests
from ddtrace.constants import MANUAL_KEEP_KEY
from ddtrace.trace import tracer
from flask import Flask

app = Flask(__name__)
PRICING_URL = os.environ["PRICING_URL"]
INVENTORY_URL = os.environ["INVENTORY_URL"]
previous_checkout = None
# Exit through SystemExit on `docker stop` so ddtrace flushes buffered spans at exit.
signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))


@app.get("/health")
def health():
    return "ok"


@app.get("/checkout/<int:order>")
def checkout(order):
    global previous_checkout
    root = tracer.current_root_span()
    if order == 2:
        # Sampling priority 2 (user keep) instead of the default auto keep.
        root.set_tag(MANUAL_KEEP_KEY)
    # A different service in the same local trace makes a multi-service chunk.
    with tracer.trace("cart.load", service="frontend-cache", resource="GET cart") as span:
        span.set_tag("cart.items", 3)
        span.set_metric("cart.total", 42.5)
    price = requests.get(f"{PRICING_URL}/price/{order}", timeout=5)
    stock = requests.get(f"{INVENTORY_URL}/stock/{order}", timeout=5)
    with tracer.trace("checkout.validate") as span:
        if previous_checkout is not None:
            span.link_span(previous_checkout, attributes={"link.reason": "previous checkout"})
        if order == 3:
            try:
                raise ValueError(f"order {order} is invalid")
            except ValueError as error:
                span.set_exc_info(type(error), error, error.__traceback__)
                # A native span event with string, int, float and bool attributes.
                span.record_exception(error, {"order.id": order, "order.total": 42.5, "retryable": False})
    previous_checkout = root.context
    return {"price": price.status_code, "stock": stock.status_code}
