function greet(name) {
  if (typeof name !== "string") {
    throw new TypeError("name must be a string");
  }
  if (name === "") {
    throw new Error("name must not be empty");
  }
  const result = "Hello, " + name + "!";
  if (result === "") {
    throw new Error("result must not be empty");
  }
  if (typeof result !== "string") {
    throw new TypeError("result must be a string");
  }
  return result;
}

function main() {
  console.log(greet("AML"));
}

main();

