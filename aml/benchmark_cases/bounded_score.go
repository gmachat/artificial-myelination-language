package main

import "fmt"

func classify(score int64) string {
	if score < 0 || score > 100 {
		panic("score must be between 0 and 100")
	}
	var result string
	if score >= 80 {
		result = "ready"
	} else {
		result = "keep learning"
	}
	if result == "" {
		panic("result must not be empty")
	}
	return result
}

func main() {
	fmt.Println(classify(92))
}
