import test_proto_pb2

out_path = "test_proto.pb"

user1 = test_proto_pb2.User(
    id="123",
    name="Alice",
    age=30,
    emails=["alice@example.com", "alice@work.com"]
)

single_binary_data = user1.SerializeToString()
with open(out_path, "wb") as f:
    f.write(single_binary_data)

print(f"Output: {out_path} size = {len(single_binary_data)} bytes")
