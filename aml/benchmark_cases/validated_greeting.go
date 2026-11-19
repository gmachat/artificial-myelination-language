package main

import (
	"fmt"
)

func greet(name string) string {
	if name == "" {
		panic("name must not be empty")
	}
	result := "Hello, " + name + "!"
	if result == "" {
		panic("result must not be empty")
	}
	return result
}

func main() {
	fmt.Println(greet("AML"))
}
