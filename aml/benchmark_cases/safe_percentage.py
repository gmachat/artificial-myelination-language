def percentage(part: float, whole: float) -> float:
    if type(part) is not float:
        raise TypeError("part must be a float")
    if type(whole) is not float:
        raise TypeError("whole must be a float")
    if whole <= 0:
        raise ValueError("whole must be positive")
    if part < 0:
        raise ValueError("part must not be negative")
    result = part / whole * 100
    if result < 0 or result > 100:
        raise ValueError("result must be between 0 and 100")
    if type(result) is not float:
        raise TypeError("result must be a float")
    return result


def main() -> None:
    print(percentage(25.0, 100.0))


if __name__ == "__main__":
    main()

