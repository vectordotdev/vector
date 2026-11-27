FROM python:3.14-alpine@sha256:5862d971d1f0ae26b258cefa4e893556b1d3aba81bd2ff30888704ad378bab67

COPY . /app
WORKDIR /app

RUN pip install --no-cache-dir --require-hashes -r requirements.txt

CMD [ "python3", "./client.py"]
