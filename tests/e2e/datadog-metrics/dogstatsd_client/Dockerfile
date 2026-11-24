FROM python:3.12-alpine@sha256:c5ba3deef00c7758561a0f03c26f44f10589263d40e732b8748e0d1f38f6dd89

COPY . /app
WORKDIR /app

RUN pip install --no-cache-dir --require-hashes -r requirements.txt

CMD [ "python3", "./client.py"]
