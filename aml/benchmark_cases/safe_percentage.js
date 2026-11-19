function percentage(part, whole) {
  if (typeof part !== "number" || !Number.isFinite(part)) {
    throw new TypeError("part must be a finite number");
  }
  if (typeof whole !== "number" || !Number.isFinite(whole)) {
    throw new TypeError("whole must be a finite number");
  }
  if (whole <= 0) {
    throw new Error("whole must be positive");
  }
  if (part < 0) {
    throw new Error("part must not be negative");
  }
  const result = part / whole * 100;
  if (result < 0 || result > 100) {
    throw new Error("result must be between 0 and 100");
  }
  if (typeof result !== "number" || !Number.isFinite(result)) {
    throw new TypeError("result must be a finite number");
  }
  return result;
}

function main() {
  console.log(percentage(25.0, 100.0));
}

main();

