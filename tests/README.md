# Running tests

In order to run tests, follow these steps:

1. Run `cd tests` if the work directory is not the current one.
2. Run `make clean` to clean artifacts and init `docker-compose` networks.
   Without it (or running any other single tests job) there might be issues
   with simulateneous running of tests because of a race condition when
   `docker-compose` creates multiple default networks and then cannot chose
   one among them.
3. Run `make -j$(nproc)` to run all tests and checks. Otherwise, it also
   possible to specify a job name from `Makefile` to run only it.
