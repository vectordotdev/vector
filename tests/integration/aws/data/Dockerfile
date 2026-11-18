# AWS Emulator (https://github.com/aws/aws-emulator) has no prebuilt image. Build one from the
# Linux package, as described in its "Getting started with Docker" guide.
FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal@sha256:61cf86a8168c2e0b8fc6b6badaac64968044c947718508d88fac06e61a596e6d

ARG EMULATOR_VERSION=1.0.343-1

RUN dnf install -y socat \
  && curl -fsSLo /tmp/aws-emulator.rpm \
    "https://downloads.aws.com/aws-emulator/packages/aws-emulator/LTS/aws-emulator-${EMULATOR_VERSION}-$(uname -m).rpm" \
  && rpm -i /tmp/aws-emulator.rpm \
  && rm /tmp/aws-emulator.rpm \
  && dnf clean all \
  && mkdir -p /home/emulator/.aws/emulator \
  && chown -R 1000 /home/emulator

COPY entrypoint.sh /entrypoint.sh

USER 1000
ENV HOME=/home/emulator
ENV EMULATOR_TELEMETRY=off
EXPOSE 4566

ENTRYPOINT ["/entrypoint.sh"]
