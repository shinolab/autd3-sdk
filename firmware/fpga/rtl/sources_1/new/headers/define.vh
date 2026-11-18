`define ASSERT_EQ(expected, actual) \
if (expected !== actual) begin \
  $error("%s:%d: expected is %s, but actual is %s", `__FILE__, `__LINE__, $sformatf("%0d", expected), $sformatf("%0d", actual));\
  $finish();\
end
