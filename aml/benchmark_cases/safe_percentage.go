package main

import (
	"fmt"
	"math"
)

func percentage(part float64, whole float64) float64 {
	if math.IsNaN(part) || math.IsInf(part, 0) {
		panic("part must be finite")
	}
	if math.IsNaN(whole) || math.IsInf(whole, 0) {
		panic("whole must be finite")
	}
	if whole <= 0 {
		panic("whole must be positive")
	}
	if part < 0 {
		panic("part must not be negative")
	}
	result := part / whole * 100
	if result < 0 || result > 100 {
		panic("result must be between 0 and 100")
	}
	return result
}

func main() {
	fmt.Println(percentage(25.0, 100.0))
}
