# Generate protobuf test file

* After modifying a protobuf file e.g. `test_protobuf3.proto`, it needs to be recompiled.
* There are many ways to create protobuf files. We are using `generate_example.py` here.

```shell
protoc -I ./ -o test_protobuf3.desc ./test_protobuf3.proto
pip install protobuf
protoc --python_out=. ./test_protobuf3.proto
python generate_example.py
```
