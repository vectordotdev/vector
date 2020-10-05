package metadata

#Fields: [Name=string]: #Fields | _

remap: {
  errors: [Name=string]: {
    description: string
    name: Name
  }

  functions: [Name=string]: {
    arguments: [
      ...{
        required: bool,

        if !required {
          name: string
        }

        type: "float" | "int" | "string"
      }
    ]
    category: "coerce" | "parse"
    description: string
    examples: [
      {
        title: string
        input: #Fields
        source: string
        output: #Fields
      },
      ...
    ]
    name: Name
  }
}
