#!/bin/bash

# Set up folders and files needed for each bind instance
mkdir -p /bind1/etc/bind /bind1/usr/share/dns /bind1/var/lib/bind \
         /bind2/etc/bind /bind2/usr/share/dns /bind2/var/lib/bind \
         /bind3/etc/bind /bind3/usr/share/dns /bind3/var/lib/bind \
         /bind4/etc/bind /bind4/usr/share/dns /bind4/var/lib/bind
cp -r /etc/bind/* /bind1/etc/bind/
cp /usr/share/dns/root.hints /bind1/usr/share/dns/
cp /var/lib/bind/db.example.com /bind1/var/lib/bind/
cp -r /etc/bind/* /bind2/etc/bind/
cp /usr/share/dns/root.hints /bind2/usr/share/dns/
cp /var/lib/bind/db.example.com /bind2/var/lib/bind/
cp -r /etc/bind/* /bind3/etc/bind/
cp /usr/share/dns/root.hints /bind3/usr/share/dns/
cp /var/lib/bind/db.example.com /bind3/var/lib/bind/
cp -r /etc/bind/* /bind4/etc/bind/
cp /usr/share/dns/root.hints /bind4/usr/share/dns/
cp /var/lib/bind/db.example.com /bind4/var/lib/bind/

# Apply named.conf.options.template to each bind instance
mv /bind1/etc/bind/named.conf.options.template /bind1/etc/bind/named.conf.options
mv /bind2/etc/bind/named.conf.options.template /bind2/etc/bind/named.conf.options
mv /bind3/etc/bind/named.conf.options.template /bind3/etc/bind/named.conf.options
mv /bind4/etc/bind/named.conf.options.template /bind4/etc/bind/named.conf.options
file1="/bind1/etc/bind/named.conf.options"
file2="/bind2/etc/bind/named.conf.options"
file3="/bind3/etc/bind/named.conf.options"
file4="/bind4/etc/bind/named.conf.options"
sed -i "s/dnstap.sock#/dnstap.sock1/" $file1
sed -i "s/port #/port 9001/" $file1
sed -i "s/dnstap.sock#/dnstap.sock2/" $file2
sed -i "s/port #/port 9002/" $file2
sed -i "s/dnstap.sock#/dnstap.sock3/" $file3
sed -i "s/port #/port 9003/" $file3
sed -i "s/dnstap.sock#/dnstap.sock4/" $file4
sed -i "s/port #/port 9004/" $file4

# Start bind4 instance to keep docker running
# Will bring up actual bind instances for IT after starting vector
/usr/sbin/named -g -p 8004 -t /bind4
