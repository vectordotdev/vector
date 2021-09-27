FROM docker.io/library/debian:bullseye

RUN apt-get update
RUN DEBIAN_FRONTEND=noninteractive apt-get -y --no-install-recommends install apt-utils
RUN DEBIAN_FRONTEND=noninteractive apt-get -y --no-install-recommends install apt-transport-https lsb-release ca-certificates curl
RUN DEBIAN_FRONTEND=noninteractive apt-get -y --no-install-recommends install wget
RUN wget -O /etc/apt/trusted.gpg.d/bind.gpg https://packages.sury.org/bind/apt.gpg
RUN sh -c 'echo "deb https://packages.sury.org/bind/ $(lsb_release -sc) main" > /etc/apt/sources.list.d/bind.list'
RUN apt-get update
RUN DEBIAN_FRONTEND=noninteractive apt-get -y --no-install-recommends install bind9 bind9utils
RUN DEBIAN_FRONTEND=noninteractive apt-get -y --no-install-recommends install dnsutils

ADD named.conf.local /etc/bind/
ADD db.example.com /var/lib/bind/
ADD nsupdate.txt /bind3/etc/bind/
ADD named.conf.options.template /etc/bind/
ADD configure_bind.sh /etc/bind/
RUN chmod +x /etc/bind/configure_bind.sh
CMD ["/etc/bind/configure_bind.sh"]
