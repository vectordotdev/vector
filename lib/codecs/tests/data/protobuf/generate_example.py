from test_protobuf3_pb2 import Person
from google.protobuf import text_format


def create_a_person_pb():
    person = Person()

    person.name = "John Doe"
    person.id = 1234
    person.email = "johndoe@example.com"

    mobile_phone = person.phones.add()
    mobile_phone.number = "1234"
    mobile_phone.type = Person.PhoneType.MOBILE

    home_phone = person.phones.add()
    home_phone.number = "5678"
    home_phone.type = Person.PhoneType.HOME

    person.data["location"] = "unknown"

    with open("a_person_proto3.pb", "wb") as file:
        file.write(person.SerializeToString())

    debug_string = text_format.MessageToString(person)
    with open("a_person_proto3_debug.txt", "w") as text_file:
        text_file.write(debug_string)


if __name__ == "__main__":
    create_a_person_pb()
