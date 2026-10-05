"""Bounded transactions using the pinned demo's instrumented Locust user."""

import os
from pathlib import Path

from locust import task
from locustfile import WebsiteUser
from opentelemetry import trace


class DemoUser(WebsiteUser):
    @task
    def transactions(self):
        for _ in range(3):
            self.get_ads()
            self.get_recommendations()
            self.view_cart()
            self.checkout_multi()

        # The catalog's not-found RPC exercises real error spans without a fault flag.
        with self.tracer.start_as_current_span("user_missing_product"):
            with self.client.get(
                "/api/products/vector-e2e-missing", catch_response=True
            ) as response:
                if response.status_code < 400:
                    response.failure("missing product unexpectedly succeeded")
                    self.environment.process_exit_code = 1
                else:
                    response.success()

        trace.get_tracer_provider().shutdown()
        directory = Path("/output/opentelemetry-demo") / os.environ["CONFIG_INGRESS_EXPORTER"]
        (directory / "workload-complete").touch()
        self.environment.runner.quit()


# Locust adds inherited tasks during class creation, even when tasks = [].
DemoUser.tasks = [DemoUser.transactions]
