#!/bin/bash

# Set up folders and files needed for each bind instance
mkdir -p /bind1/etc/bind /bind1/usr/share/dns /bind1/var/lib/bind \
         /bind2/etc/bind /bind2/usr/share/dns /bind2/var/lib/bind \
         /bind3/etc/bind /bind3/usr/share/dns /bind3/var/lib/bind
cp -r /etc/bind/* /bind1/etc/bind/
cp /usr/share/dns/root.hints /bind1/usr/share/dns/
cp /var/lib/bind/db.example.com /bind1/var/lib/bind/
cp -r /etc/bind/* /bind2/etc/bind/
cp /usr/share/dns/root.hints /bind2/usr/share/dns/
cp /var/lib/bind/db.example.com /bind2/var/lib/bind/
cp -r /etc/bind/* /bind3/etc/bind/
cp /usr/share/dns/root.hints /bind3/usr/share/dns/
cp /var/lib/bind/db.example.com /bind3/var/lib/bind/

# Apply named.conf.options.template to each bind instance
mv /bind1/etc/bind/named.conf.options.template /bind1/etc/bind/named.conf.options
mv /bind2/etc/bind/named.conf.options.template /bind2/etc/bind/named.conf.options
mv /bind3/etc/bind/named.conf.options.template /bind3/etc/bind/named.conf.options
file1="/bind1/etc/bind/named.conf.options"
file2="/bind2/etc/bind/named.conf.options"
file3="/bind3/etc/bind/named.conf.options"
sed -i "s/dnstap.sock#/dnstap.sock1/" $file1
sed -i "s/port #/port 9001/" $file1
sed -i "s/dnstap.sock#/dnstap.sock2/" $file2
sed -i "s/port #/port 9002/" $file2
sed -i "s/dnstap.sock#/dnstap.sock3/" $file3
sed -i "s/port #/port 9003/" $file3

# Start bind instances
/usr/sbin/named -p 8001 -t /bind1
/usr/sbin/named -p 8002 -t /bind2
/usr/sbin/named -g -p 8003 -t /bind3
