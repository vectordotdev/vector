"""Run a fixed workload using the demo's instrumented Locust user.

Unlike the default random, continuous workload, this stops after three shopping
rounds and writes a completion marker so trace validation can start.
"""

import os
from pathlib import Path

import gevent
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

        trace.get_tracer_provider().shutdown()
        directory = Path("/output/opentelemetry-traces-multiservice") / os.environ["CONFIG_INGRESS_EXPORTER"]
        (directory / "workload-complete").touch()
        # Quit outside this user's task so stopping users can complete.
        gevent.spawn(self.environment.runner.quit)


# Locust adds inherited tasks during class creation, even when tasks = [].
DemoUser.tasks = [DemoUser.transactions]
