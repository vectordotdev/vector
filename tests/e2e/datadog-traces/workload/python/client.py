"""Send a fixed set of checkouts through the frontend, then exit."""

import os
import urllib.request

URL = os.environ["FRONTEND_URL"]
ORDERS = int(os.environ.get("ORDERS", "6"))

for order in range(1, ORDERS + 1):
    with urllib.request.urlopen(f"{URL}/checkout/{order}", timeout=10) as response:
        print(f"order {order}: {response.status}", flush=True)
