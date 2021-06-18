FROM docker.io/library/debian:buster

RUN apt-get update
RUN apt-get -y install apt-transport-https lsb-release ca-certificates curl
RUN apt-get -y install wget
RUN wget -O /etc/apt/trusted.gpg.d/bind.gpg https://packages.sury.org/bind/apt.gpg
RUN sh -c 'echo "deb https://packages.sury.org/bind/ $(lsb_release -sc) main" > /etc/apt/sources.list.d/bind.list'
RUN apt-get update
RUN apt-get -y install bind9 bind9utils
RUN apt-get -y install dnsutils

ADD named.conf.local /etc/bind/
ADD db.example.com /var/lib/bind/
ADD nsupdate.txt /bind3/etc/bind/
ADD named.conf.options.template /etc/bind/
ADD configure_bind.sh /etc/bind/
RUN chmod +x /etc/bind/configure_bind.sh
CMD ["/etc/bind/configure_bind.sh"]
