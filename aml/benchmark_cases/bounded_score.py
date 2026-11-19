def classify(score: int) -> str:
    if type(score) is not int:
        raise TypeError("score must be an integer")
    if score < 0 or score > 100:
        raise ValueError("score must be between 0 and 100")
    if score >= 80:
        result = "ready"
    else:
        result = "keep learning"
    if result == "":
        raise ValueError("result must not be empty")
    if type(result) is not str:
        raise TypeError("result must be a string")
    return result


def main() -> None:
    print(classify(92))


if __name__ == "__main__":
    main()

