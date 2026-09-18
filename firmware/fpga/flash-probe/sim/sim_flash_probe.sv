`timescale 1ns / 1ps
module sim_flash_probe ();

  localparam int POLL_LIMIT = 20;

  localparam logic [7:0] OP_ID = 8'h01;
  localparam logic [7:0] OP_SR = 8'h02;
  localparam logic [7:0] OP_READ = 8'h03;
  localparam logic [7:0] OP_CRC = 8'h04;
  localparam logic [7:0] OP_FSR = 8'h05;
  localparam logic [7:0] OP_ERASE = 8'h06;
  localparam logic [7:0] OP_PROGRAM = 8'h07;
  localparam logic [7:0] OP_IPROG = 8'h08;
  localparam logic [7:0] OP_WBSTAR = 8'h09;

  localparam logic [7:0] ERR_NONE = 8'h00;
  localparam logic [7:0] ERR_PROTECTED = 8'h01;
  localparam logic [7:0] ERR_TIMEOUT = 8'h02;

  logic CLK = 1'b0;
  always #19.531 CLK = ~CLK;

  logic start = 1'b0;
  logic cmd_valid = 1'b0;
  logic [7:0] cmd_op = 8'd0;
  logic [23:0] cmd_addr = 24'd0;
  logic [31:0] cmd_len = 32'd0;

  wire [63:0] result;
  wire [7:0] result_op;
  wire busy;
  wire done;
  wire sck;
  wire cs_n;
  wire mosi;
  wire miso;

  int errors = 0;

  initial begin
    #20s;
    $display("NG! sim_flash_probe: timed out");
    $finish();
  end

  wire iprog_start;
  wire iprog_reboot;
  wire [23:0] iprog_addr;
  wire [31:0] iprog_timer;

  flash_probe_core #(
      .POLL_LIMIT(POLL_LIMIT)
  ) dut (
      .CLK(CLK),
      .START(start),
      .CMD_VALID(cmd_valid),
      .CMD_OP(cmd_op),
      .CMD_ADDR(cmd_addr),
      .CMD_LEN(cmd_len),
      .RESULT(result),
      .RESULT_OP(result_op),
      .BUSY(busy),
      .DONE(done),
      .SCK(sck),
      .CS_N(cs_n),
      .MOSI(mosi),
      .MISO(miso),
      .IPROG_START(iprog_start),
      .IPROG_REBOOT(iprog_reboot),
      .IPROG_ADDR(iprog_addr),
      .IPROG_TIMER(iprog_timer)
  );

  wire icap_csib;
  wire icap_rdwrb;
  wire [31:0] icap_i;

  flash_probe_iprog iprog (
      .CLK(CLK),
      .START(iprog_start),
      .REBOOT(iprog_reboot),
      .ADDR(iprog_addr),
      .TIMER(iprog_timer),
      .ICAP_CSIB(icap_csib),
      .ICAP_RDWRB(icap_rdwrb),
      .ICAP_I(icap_i)
  );

  function automatic logic [31:0] icap_unswap(input logic [31:0] w);
    logic [31:0] s;
    for (int i = 0; i < 32; i++) begin
      s[i] = w[(i/8)*8+7-(i%8)];
    end
    return s;
  endfunction

  logic [31:0] icap_words[$];
  int icap_rdwrb_violations = 0;

  always @(posedge CLK) begin
    if (!icap_csib) begin
      icap_words.push_back(icap_unswap(icap_i));
      if (icap_rdwrb) begin
        icap_rdwrb_violations++;
      end
    end
  end

  task automatic check_icap(input string name, input logic [31:0] expected[$]);
    if (icap_words.size() != expected.size()) begin
      $display("FAIL %s: expected %0d ICAP words, got %0d", name, expected.size(), icap_words.size());
      errors++;
      return;
    end
    for (int i = 0; i < expected.size(); i++) begin
      check($sformatf("%s word %0d", name, i), 64'(icap_words[i]), 64'(expected[i]));
    end
  endtask

  sim_flash_model flash (
      .SCK (sck),
      .CS_N(cs_n),
      .MOSI(mosi),
      .MISO(miso)
  );

  task automatic send(input logic [7:0] op, input logic [23:0] addr, input logic [31:0] len);
    @(posedge CLK);
    cmd_valid <= 1'b1;
    cmd_op <= op;
    cmd_addr <= addr;
    cmd_len <= len;
    @(posedge CLK);
    cmd_valid <= 1'b0;
    @(posedge CLK);
  endtask

  task automatic wait_done();
    while (!done) begin
      @(posedge CLK);
    end
  endtask

  task automatic exec(input logic [7:0] op, input logic [23:0] addr, input logic [31:0] len);
    send(op, addr, len);
    wait_done();
  endtask

  task automatic check(input string name, input logic [63:0] actual, input logic [63:0] expected);
    if (actual !== expected) begin
      $display("FAIL %s: expected %h, got %h", name, expected, actual);
      errors++;
    end
  endtask

  function automatic logic [31:0] crc_byte(input logic [31:0] c, input logic [7:0] d);
    logic [31:0] x;
    x = c ^ {24'd0, d};
    for (int b = 0; b < 8; b++) begin
      if (x[0]) begin
        x = (x >> 1) ^ 32'hEDB88320;
      end else begin
        x = x >> 1;
      end
    end
    return x;
  endfunction

  function automatic logic [31:0] reference_crc(input int unsigned addr, input int unsigned len);
    logic [31:0] c;
    c = 32'hFFFFFFFF;
    for (int unsigned i = 0; i < len; i++) begin
      c = crc_byte(c, flash.peek(addr + i));
    end
    return ~c;
  endfunction

  function automatic logic [31:0] erased_crc(input int unsigned len);
    logic [31:0] c;
    c = 32'hFFFFFFFF;
    for (int unsigned i = 0; i < len; i++) begin
      c = crc_byte(c, 8'hFF);
    end
    return ~c;
  endfunction

  function automatic logic [31:0] pattern_crc(input int unsigned addr, input int unsigned len, input logic [7:0] seed);
    logic [31:0] x;
    logic [31:0] c;
    x = {seed, addr[23:0]} ^ 32'h9E3779B9;
    if (x == 32'd0) begin
      x = 32'd1;
    end
    c = 32'hFFFFFFFF;
    for (int unsigned i = 0; i < len; i++) begin
      x = x ^ (x << 13);
      x = x ^ (x >> 17);
      x = x ^ (x << 5);
      c = crc_byte(c, x[7:0]);
    end
    return ~c;
  endfunction

  function automatic logic [63:0] reference_read(input int unsigned addr);
    logic [63:0] v;
    v = 64'd0;
    for (int unsigned i = 0; i < 8; i++) begin
      v = {v[55:0], flash.peek(addr + i)};
    end
    return v;
  endfunction

  task automatic fill_random(input int unsigned addr, input int unsigned len);
    for (int unsigned i = 0; i < len; i++) begin
      flash.mem[addr+i] = 8'($urandom);
    end
  endtask

  initial begin
    string vector;
    int selects_before;
    int erases_before;
    int programs_before;
    logic [31:0] low_before;
    logic [31:0] guard_before;
    logic [7:0] after_erase_before;

    fill_random(32'h0, 32'h10000);
    fill_random(32'h7FF000, 32'h1000);
    fill_random(32'hF00000, 32'h4000);
    vector = "123456789";
    for (int i = 0; i < vector.len(); i++) begin
      flash.mem[32'h2000+i] = vector[i];
    end

    repeat (10) @(posedge CLK);
    check("cs before start", 64'(cs_n), 64'd1);
    start <= 1'b1;
    @(posedge CLK);
    while (busy) begin
      @(posedge CLK);
    end
    check("no select during startup", 64'(flash.selects), 64'd0);

    exec(OP_ID, 24'd0, 32'd0);
    check("id op", 64'(result_op), 64'(OP_ID));
    check("id", result, 64'h20BA18);

    exec(OP_SR, 24'd0, 32'd0);
    check("sr", result, 64'h00);

    exec(OP_FSR, 24'd0, 32'd0);
    check("fsr", result, 64'h80);

    exec(OP_READ, 24'h001234, 32'd0);
    check("read", result, reference_read(32'h1234));

    exec(OP_READ, 24'h00FFFC, 32'd0);
    check("read boundary", result, reference_read(32'hFFFC));

    exec(OP_CRC, 24'h002000, 32'd9);
    check("crc check value", result, 64'hCBF43926);

    exec(OP_CRC, 24'h000100, 32'd3000);
    check("crc random", result, 64'(reference_crc(32'h100, 3000)));

    selects_before = flash.selects;
    exec(OP_CRC, 24'h000100, 32'd0);
    check("crc empty", result, 64'd0);

    exec(8'h55, 24'd0, 32'd0);
    check("invalid op result", result, 64'd0);
    check("empty crc and invalid op do not select", 64'(flash.selects), 64'(selects_before));

    send(OP_CRC, 24'h000000, 32'd500);
    send(OP_ID, 24'd0, 32'd0);
    wait_done();
    check("busy ignores command op", 64'(result_op), 64'(OP_CRC));
    check("busy ignores command", result, 64'(reference_crc(0, 500)));

    low_before = reference_crc(32'h0, 32'h10000);
    guard_before = reference_crc(32'h7FF000, 32'h1000);

    selects_before = flash.selects;
    exec(OP_ERASE, 24'h7FF000, 32'h1000);
    check("erase below base", 64'(result[63:56]), 64'(ERR_PROTECTED));
    exec(OP_ERASE, 24'h7FFFFF, 32'h2);
    check("erase straddling base", 64'(result[63:56]), 64'(ERR_PROTECTED));
    exec(OP_ERASE, 24'hFFF000, 32'h2000);
    check("erase past end", 64'(result[63:56]), 64'(ERR_PROTECTED));
    exec(OP_ERASE, 24'h800000, 32'hFFFFFFFF);
    check("erase huge length", 64'(result[63:56]), 64'(ERR_PROTECTED));
    exec(OP_PROGRAM, 24'h7FFFF0, {8'h11, 24'h10});
    check("program below base", 64'(result[63:56]), 64'(ERR_PROTECTED));
    exec(OP_PROGRAM, 24'hFFFFF0, {8'h11, 24'h20});
    check("program past end", 64'(result[63:56]), 64'(ERR_PROTECTED));
    exec(OP_PROGRAM, 24'hF01000, {8'h11, 24'h0});
    check("program empty", 64'(result[63:56]), 64'(ERR_NONE));
    exec(OP_ERASE, 24'hF01000, 32'h0);
    check("erase empty", 64'(result[63:56]), 64'(ERR_NONE));
    check("rejected and empty writes do not select", 64'(flash.selects), 64'(selects_before));

    after_erase_before = flash.peek(32'hF03000);
    erases_before = flash.erases;
    exec(OP_ERASE, 24'hF00123, 32'h1F00);
    check("erase error", 64'(result[63:56]), 64'(ERR_NONE));
    check("erase final sr", 64'(result[7:0]), 64'h00);
    check("erase sector count", 64'(flash.erases - erases_before), 64'd3);
    check("erased sectors", 64'(reference_crc(32'hF00000, 32'h3000)), 64'(erased_crc(32'h3000)));
    check("sector after erase untouched", 64'(flash.peek(32'hF03000)), 64'(after_erase_before));

    programs_before = flash.programs;
    exec(OP_PROGRAM, 24'hF000F0, {8'h5A, 24'd700});
    check("program error", 64'(result[63:56]), 64'(ERR_NONE));
    check("program page count", 64'(flash.programs - programs_before), 64'd4);
    check("programmed pattern", 64'(reference_crc(32'hF000F0, 700)), 64'(pattern_crc(32'hF000F0, 700, 8'h5A)));
    check("byte before program", 64'(flash.peek(32'hF000EF)), 64'hFF);
    check("byte after program", 64'(flash.peek(32'hF000F0 + 700)), 64'hFF);

    exec(OP_CRC, 24'hF000F0, 32'd700);
    check("crc of programmed pattern", result, 64'(pattern_crc(32'hF000F0, 700, 8'h5A)));

    exec(OP_SR, 24'd0, 32'd0);
    check("sr after writes", result, 64'h00);

    check("low region untouched", 64'(reference_crc(32'h0, 32'h10000)), 64'(low_before));
    check("guard region untouched", 64'(reference_crc(32'h7FF000, 32'h1000)), 64'(guard_before));

    check("no icap activity before iprog", 64'(icap_words.size()), 64'd0);

    selects_before = flash.selects;
    exec(OP_IPROG, 24'h800000, 32'd0);
    repeat (20) @(posedge CLK);
    check_icap("iprog", '{32'hFFFFFFFF, 32'hAA995566, 32'h20000000, 32'h30020001, 32'h00800000, 32'h30008001, 32'h0000000F, 32'h20000000});
    icap_words.delete();

    exec(OP_IPROG, 24'hABCDEF, 32'h40000FFF);
    repeat (20) @(posedge CLK);
    check_icap("iprog with timer",
               '{
                   32'hFFFFFFFF,
                   32'hAA995566,
                   32'h20000000,
                   32'h30022001,
                   32'h40000FFF,
                   32'h30020001,
                   32'h00ABCDEF,
                   32'h30008001,
                   32'h0000000F,
                   32'h20000000
               });
    icap_words.delete();

    exec(OP_WBSTAR, 24'h800000, 32'd0);
    repeat (20) @(posedge CLK);
    check("wbstar op", 64'(result_op), 64'(OP_WBSTAR));
    check_icap("wbstar only", '{32'hFFFFFFFF, 32'hAA995566, 32'h20000000, 32'h30020001, 32'h00800000, 32'h30008001, 32'h0000000D, 32'h20000000});
    icap_words.delete();

    exec(OP_WBSTAR, 24'h123456, 32'h40000FFF);
    repeat (20) @(posedge CLK);
    check_icap("wbstar and timer only",
               '{
                   32'hFFFFFFFF,
                   32'hAA995566,
                   32'h20000000,
                   32'h30022001,
                   32'h40000FFF,
                   32'h30020001,
                   32'h00123456,
                   32'h30008001,
                   32'h0000000D,
                   32'h20000000
               });
    icap_words.delete();

    exec(OP_IPROG, 24'h800000, 32'd0);
    repeat (20) @(posedge CLK);
    check_icap("iprog after wbstar only",
               '{32'hFFFFFFFF, 32'hAA995566, 32'h20000000, 32'h30020001, 32'h00800000, 32'h30008001, 32'h0000000F, 32'h20000000});
    icap_words.delete();

    check("iprog does not touch the flash", 64'(flash.selects), 64'(selects_before));
    check("icap rdwrb stays write", 64'(icap_rdwrb_violations), 64'd0);

    exec(OP_ID, 24'd0, 32'd0);
    repeat (20) @(posedge CLK);
    check("other ops do not drive icap", 64'(icap_words.size()), 64'd0);

    flash.stuck_wip = 1'b1;
    exec(OP_ERASE, 24'hF08000, 32'h1000);
    check("erase timeout", 64'(result[63:56]), 64'(ERR_TIMEOUT));
    check("erase timeout sr", 64'(result[7:0]), 64'h01);

    if (flash.violations != 0) begin
      $display("FAIL flash model reported %0d violation(s)", flash.violations);
      errors++;
    end

    if (errors == 0) begin
      $display("OK! sim_flash_probe");
    end else begin
      $display("NG! sim_flash_probe: %0d error(s)", errors);
    end
    $finish();
  end

endmodule
