def classify(score)
  raise TypeError, "score must be an integer" unless score.instance_of?(Integer)
  raise ArgumentError, "score must be between 0 and 100" if score < 0 || score > 100
  result = score >= 80 ? "ready" : "keep learning"
  raise RuntimeError, "result must not be empty" if result.empty?
  raise TypeError, "result must be a string" unless result.instance_of?(String)
  result
end

def main
  puts classify(92)
end

main
