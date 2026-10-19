FROM ubuntu:24.04@sha256:68ee22004a9109b44973313b02ff7b1de250f1de0f5ecb628d73f579aaff33ac
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    apt-utils ca-certificates createrepo-c dpkg-dev gnupg python3 python3-venv rpm \
    && rm -rf /var/lib/apt/lists/*
COPY requirements.txt /app/requirements.txt
RUN python3 -m venv /opt/publisher && /opt/publisher/bin/pip install \
    --no-cache-dir --require-hashes --only-binary=:all: -r /app/requirements.txt
COPY *.py /app/
COPY tests /app/tests
WORKDIR /app
ENTRYPOINT ["/opt/publisher/bin/python", "/app/engine.py"]
