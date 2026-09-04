$TTL 3600
@ SOA ns.example.com. postmaster.no.email.please. ( 636817096 3600 600 2592000 3600 )
@ 86400 NS ns.example.com.
ns.example.com. 86400 A 172.17.0.2
h1 A 10.0.0.11
