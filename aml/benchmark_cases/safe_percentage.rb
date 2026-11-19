def percentage(part, whole)
  raise TypeError, "part must be a finite float" unless part.instance_of?(Float) && part.finite?
  raise TypeError, "whole must be a finite float" unless whole.instance_of?(Float) && whole.finite?
  raise ArgumentError, "whole must be positive" unless whole > 0
  raise ArgumentError, "part must not be negative" unless part >= 0
  result = part / whole * 100
  raise RuntimeError, "result must be between 0 and 100" unless result >= 0 && result <= 100
  raise TypeError, "result must be a finite float" unless result.instance_of?(Float) && result.finite?
  result
end

def main
  puts percentage(25.0, 100.0)
end

main
