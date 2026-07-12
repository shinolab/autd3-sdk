`timescale 1ns / 1ps
module sim_helper_random;

  function automatic longint unsigned range(longint unsigned max, longint unsigned min);
    automatic longint unsigned span = max - min + 1;
    automatic longint unsigned r = {$urandom(), $urandom()};
    range = (span == 0) ? (r + min) : ((r % span) + min);
  endfunction

endmodule
