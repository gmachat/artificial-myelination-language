function classify(score) {
  if (!Number.isInteger(score)) {
    throw new TypeError("score must be an integer");
  }
  if (score < 0 || score > 100) {
    throw new Error("score must be between 0 and 100");
  }
  let result;
  if (score >= 80) {
    result = "ready";
  } else {
    result = "keep learning";
  }
  if (result === "") {
    throw new Error("result must not be empty");
  }
  if (typeof result !== "string") {
    throw new TypeError("result must be a string");
  }
  return result;
}

function main() {
  console.log(classify(92));
}

main();

