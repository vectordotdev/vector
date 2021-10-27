# Test Data

* [`./GeoIP2-City-Test.mmdb`](https://github.com/maxmind/MaxMind-DB/tree/6e99232bb6a70d5169ecc96ed0614a52017ff654/test-data)
* `./Intermediate_CA.csr`
  * `openssl req -config Intermediate_CA.cfg -new -sha256 -key Intermediate_CA.key -out Intermediate_CA.csr`
* `./Intermediate_CA.crt`
  * `openssl ca -config Vector_CA.cfg -extensions v3_intermediate_ca -days 3287 -notext -md sha256 -in Intermediate_CA.csr -out Intermediate_CA.crt`
* `./Crt_from_intermediate.csr`
  * `openssl req -config Crt_from_intermediate.cfg -new -sha256 -key Crt_from_intermediate.key -out Crt_from_intermediate.csr`
* `./Crt_from_intermediate.crt`
  * `openssl ca -config Intermediate_CA.cfg -days 3287 -notext -md sha256 -in Crt_from_intermediate.csr -out Crt_from_intermediate.crt`
