def greet(name)
  raise TypeError, "name must be a string" unless name.instance_of?(String)
  raise ArgumentError, "name must not be empty" if name.empty?
  result = "Hello, " + name + "!"
  raise RuntimeError, "result must not be empty" if result.empty?
  raise TypeError, "result must be a string" unless result.instance_of?(String)
  result
end

def main
  puts greet("AML")
end

main
