def greet(name: str) -> str:
    if type(name) is not str:
        raise TypeError("name must be a string")
    if name == "":
        raise ValueError("name must not be empty")
    result = "Hello, " + name + "!"
    if result == "":
        raise ValueError("result must not be empty")
    if type(result) is not str:
        raise TypeError("result must be a string")
    return result


def main() -> None:
    print(greet("AML"))


if __name__ == "__main__":
    main()

