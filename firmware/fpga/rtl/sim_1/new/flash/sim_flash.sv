`timescale 1ns / 1ps
module sim_flash ();

  `include "define.vh"

  import params::*;

  localparam int POLL_INTERVAL = 16;
  localparam int POLL_LIMIT = 20;
  localparam int MAX_STATUS_POLLS = 2000000;

  logic CLK;
  logic locked;

  sim_helper_bram sim_helper_bram ();

  sim_helper_clk sim_helper_clk (
      .CLK(CLK),
      .LOCKED(locked),
      .SYS_TIME()
  );

  cnt_bus_if cnt_bus ();
  phase_corr_bus_if phase_corr_bus ();
  modulation_bus_if mod_bus ();
  emission_bus_if emission_bus ();
  pwe_table_bus_if pwe_table_bus ();
  output_mask_bus_if output_mask_bus ();
  flash_bus_if flash_bus ();

  memory memory (
      .CLK(CLK),
      .MEM_BUS(sim_helper_bram.memory_bus.bram_port),
      .CNT_BUS(cnt_bus.in_port),
      .PHASE_CORR_BUS(phase_corr_bus.in_port),
      .OUTPUT_MASK_BUS(output_mask_bus.in_port),
      .MOD_BUS(mod_bus.in_port),
      .EMISSION_BUS(emission_bus.in_port),
      .PWE_TABLE_BUS(pwe_table_bus.in_port),
      .FLASH_BUS(flash_bus.host_port)
  );

  assign cnt_bus.WE   = 1'b0;
  assign cnt_bus.ADDR = 8'd0;
  assign cnt_bus.DIN  = 16'd0;

  logic eos = 1'b0;
  logic [31:0] usr_access = FlashUsrAccessUpdate;
  wire sck;
  wire cs_n;
  wire mosi;
  wire miso;
  wire icap_csib;
  wire [31:0] icap_i;

  flash #(
      .POLL_INTERVAL(POLL_INTERVAL),
      .POLL_LIMIT(POLL_LIMIT)
  ) flash (
      .CLK(CLK),
      .LOCKED(locked),
      .EOS(eos),
      .USR_ACCESS(usr_access),
      .FLASH_BUS(flash_bus.core_port),
      .SCK(sck),
      .CS_N(cs_n),
      .MOSI(mosi),
      .MISO(miso),
      .ICAP_CSIB(icap_csib),
      .ICAP_I(icap_i)
  );

  sim_helper_flash #(
      .ERASE_NS  (20000),
      .PROGRAM_NS(5000)
  ) model (
      .SCK (sck),
      .CS_N(cs_n),
      .MOSI(mosi),
      .MISO(miso)
  );

  function automatic logic [31:0] icap_unswap(input logic [31:0] w);
    logic [31:0] s;
    for (int i = 0; i < 32; i++) begin
      s[i] = w[(i/8)*8+7-(i%8)];
    end
    return s;
  endfunction

  logic [31:0] icap_words[$];

  always @(posedge CLK) begin
    if (!icap_csib) begin
      icap_words.push_back(icap_unswap(icap_i));
    end
  end

  function automatic logic [31:0] crc32(input logic [7:0] bytes[$]);
    logic [31:0] c;
    c = 32'hFFFFFFFF;
    foreach (bytes[k]) begin
      c = c ^ {24'd0, bytes[k]};
      for (int b = 0; b < 8; b++) begin
        c = c[0] ? ((c >> 1) ^ 32'hEDB88320) : (c >> 1);
      end
    end
    return ~c;
  endfunction

  task automatic reg_write(input logic [7:0] a, input logic [15:0] v);
    sim_helper_bram.bram_write(BRAM_SELECT_CONTROLLER, {BRAM_CNT_SELECT_FLASH, a}, v);
  endtask

  task automatic reg_read(input logic [7:0] a, output logic [15:0] v);
    sim_helper_bram.bram_read(BRAM_SELECT_CONTROLLER, {BRAM_CNT_SELECT_FLASH, a}, v);
  endtask

  task automatic buf_write(input logic [7:0] bytes[$]);
    logic [7:0] lo;
    logic [7:0] hi;
    for (int i = 0; i < (bytes.size() + 1) / 2; i++) begin
      lo = bytes[2*i];
      hi = (2 * i + 1 < bytes.size()) ? bytes[2*i+1] : 8'hFF;
      sim_helper_bram.bram_write(BRAM_SELECT_CONTROLLER, {5'b00010, i[8:0]}, {hi, lo});
    end
  endtask

  task automatic start_cmd(input logic [7:0] op, input logic [23:0] addr, input logic [23:0] len);
    reg_write(ADDR_FLASH_ADDR_0, addr[15:0]);
    reg_write(ADDR_FLASH_ADDR_1, {8'd0, addr[23:16]});
    reg_write(ADDR_FLASH_LEN_0, len[15:0]);
    reg_write(ADDR_FLASH_LEN_1, {8'd0, len[23:16]});
    reg_write(ADDR_FLASH_CMD, {8'd0, op});
  endtask

  task automatic wait_cmd(output logic [7:0] err, output logic [31:0] result);
    logic [15:0] status;
    logic [15:0] lo;
    logic [15:0] hi;
    int polls;
    polls  = 0;
    status = 16'h0001;
    while (status[0]) begin
      @(posedge CLK);
      reg_read(ADDR_FLASH_STATUS, status);
      polls++;
      if (polls > MAX_STATUS_POLLS) begin
        $error("flash command did not finish");
        $finish();
      end
    end
    err = status[15:8];
    reg_read(ADDR_FLASH_RESULT_0, lo);
    reg_read(ADDR_FLASH_RESULT_1, hi);
    result = {hi, lo};
  endtask

  task automatic run_cmd(input logic [7:0] op, input logic [23:0] addr, input logic [23:0] len, output logic [7:0] err, output logic [31:0] result);
    start_cmd(op, addr, len);
    wait_cmd(err, result);
  endtask

  task automatic expect_err(input logic [7:0] op, input logic [23:0] addr, input logic [23:0] len, input logic [7:0] expected);
    logic [ 7:0] err;
    logic [31:0] result;
    run_cmd(op, addr, len, err, result);
    `ASSERT_EQ(expected, err);
  endtask

  logic [7:0] golden[$];
  logic [7:0] stale[$];
  logic [7:0] data[$];
  logic [7:0] err;
  logic [31:0] result;
  logic [15:0] value;
  int erases_before;
  int programs_before;
  logic [31:0] expected_icap[$];

  initial begin
    @(posedge locked);
    repeat (16) @(posedge CLK);

    for (int i = 0; i < 300; i++) begin
      golden.push_back($urandom_range(0, 255));
      model.mem[32'h000100+i] = golden[i];
    end
    for (int i = 0; i < 16; i++) begin
      stale.push_back($urandom_range(0, 254));
      model.mem[32'h80FFF8+i] = stale[i];
    end

    start_cmd(FLASH_OP_READ_ID, 24'd0, 24'd0);
    repeat (64) @(posedge CLK);
    reg_read(ADDR_FLASH_STATUS, value);
    `ASSERT_EQ(1'b1, value[0]);
    `ASSERT_EQ(0, model.selects);
    eos = 1'b1;
    wait_cmd(err, result);
    `ASSERT_EQ(FLASH_ERR_NONE, err);
    `ASSERT_EQ(32'h0020BA18, result);

    reg_read(ADDR_FLASH_USR_ACCESS_0, value);
    `ASSERT_EQ(FlashUsrAccessUpdate[15:0], value);
    reg_read(ADDR_FLASH_USR_ACCESS_1, value);
    `ASSERT_EQ(FlashUsrAccessUpdate[31:16], value);
    reg_read(ADDR_FLASH_ADDR_1, value);
    `ASSERT_EQ(16'd0, value);

    run_cmd(FLASH_OP_CRC32, 24'h000100, 24'd300, err, result);
    `ASSERT_EQ(FLASH_ERR_NONE, err);
    `ASSERT_EQ(crc32(golden), result);
    run_cmd(FLASH_OP_CRC32, 24'h000000, 24'd0, err, result);
    `ASSERT_EQ(FLASH_ERR_NONE, err);
    `ASSERT_EQ(32'd0, result);
    expect_err(FLASH_OP_CRC32, 24'hFFFFF0, 24'h000011, FLASH_ERR_INVALID);
    expect_err(8'h7F, 24'h800000, 24'd1, FLASH_ERR_INVALID);
    expect_err(8'h00, 24'h800000, 24'd1, FLASH_ERR_INVALID);

    expect_err(FLASH_OP_ERASE, 24'h7F0000, 24'h020000, FLASH_ERR_PROTECTED);
    expect_err(FLASH_OP_ERASE, 24'h7FFFFF, 24'h000001, FLASH_ERR_PROTECTED);
    expect_err(FLASH_OP_ERASE, 24'h000000, 24'h000001, FLASH_ERR_PROTECTED);
    expect_err(FLASH_OP_ERASE, 24'hFF0000, 24'h010001, FLASH_ERR_PROTECTED);
    expect_err(FLASH_OP_PROGRAM, 24'h7FFF00, 24'h000100, FLASH_ERR_PROTECTED);
    expect_err(FLASH_OP_PROGRAM, 24'h7FFFFF, 24'h000002, FLASH_ERR_PROTECTED);
    expect_err(FLASH_OP_PROGRAM, 24'hFFFF00, 24'h000101, FLASH_ERR_PROTECTED);
    expect_err(FLASH_OP_PROGRAM, 24'h800000, 24'h000401, FLASH_ERR_INVALID);
    `ASSERT_EQ(0, model.erases);
    `ASSERT_EQ(0, model.programs);

    run_cmd(FLASH_OP_ERASE, 24'h800000, 24'h000100 + 24'd700, err, result);
    `ASSERT_EQ(FLASH_ERR_NONE, err);
    `ASSERT_EQ(1, model.erases);
    for (int i = 0; i < 8; i++) begin
      `ASSERT_EQ(8'hFF, model.peek(32'h80FFF8 + i));
      `ASSERT_EQ(stale[8+i], model.peek(32'h810000 + i));
    end

    run_cmd(FLASH_OP_ERASE, 24'h80FFFF, 24'd2, err, result);
    `ASSERT_EQ(FLASH_ERR_NONE, err);
    `ASSERT_EQ(3, model.erases);
    for (int i = 0; i < 8; i++) begin
      `ASSERT_EQ(8'hFF, model.peek(32'h810000 + i));
    end

    run_cmd(FLASH_OP_ERASE, 24'hFF0000, 24'h010000, err, result);
    `ASSERT_EQ(FLASH_ERR_NONE, err);
    `ASSERT_EQ(4, model.erases);
    run_cmd(FLASH_OP_ERASE, 24'h900000, 24'd0, err, result);
    `ASSERT_EQ(FLASH_ERR_NONE, err);
    `ASSERT_EQ(4, model.erases);

    for (int i = 0; i < FlashBufBytes; i++) begin
      data.push_back($urandom_range(0, 255));
    end
    buf_write(data);
    run_cmd(FLASH_OP_PROGRAM, 24'h8001F0, 24'd1000, err, result);
    `ASSERT_EQ(FLASH_ERR_NONE, err);
    `ASSERT_EQ(5, model.programs);
    for (int i = 0; i < 1000; i++) begin
      `ASSERT_EQ(data[i], model.peek(32'h8001F0 + i));
    end
    `ASSERT_EQ(8'hFF, model.peek(32'h8001F0 + 1000));
    `ASSERT_EQ(8'hFF, model.peek(32'h8001EF));
    data = data[0:999];
    run_cmd(FLASH_OP_CRC32, 24'h8001F0, 24'd1000, err, result);
    `ASSERT_EQ(FLASH_ERR_NONE, err);
    `ASSERT_EQ(crc32(data), result);

    data = {};
    for (int i = 0; i < FlashBufBytes; i++) begin
      data.push_back($urandom_range(0, 255));
    end
    buf_write(data);
    run_cmd(FLASH_OP_PROGRAM, 24'hFFFC00, 24'd1024, err, result);
    `ASSERT_EQ(FLASH_ERR_NONE, err);
    `ASSERT_EQ(9, model.programs);
    run_cmd(FLASH_OP_CRC32, 24'hFFFC00, 24'd1024, err, result);
    `ASSERT_EQ(crc32(data), result);

    programs_before = model.programs;
    run_cmd(FLASH_OP_PROGRAM, 24'h900000, 24'd0, err, result);
    `ASSERT_EQ(FLASH_ERR_NONE, err);
    `ASSERT_EQ(programs_before, model.programs);

    erases_before = model.erases;
    start_cmd(FLASH_OP_ERASE, 24'hA00000, 24'h010000);
    reg_read(ADDR_FLASH_STATUS, value);
    `ASSERT_EQ(1'b1, value[0]);
    reg_write(ADDR_FLASH_CMD, {8'd0, FLASH_OP_READ_ID});
    reg_read(ADDR_FLASH_CMD, value);
    `ASSERT_EQ({8'd0, FLASH_OP_ERASE}, value);
    wait_cmd(err, result);
    `ASSERT_EQ(FLASH_ERR_NONE, err);
    `ASSERT_EQ(32'd0, result);
    `ASSERT_EQ(erases_before + 1, model.erases);
    repeat (64) @(posedge CLK);
    reg_read(ADDR_FLASH_STATUS, value);
    `ASSERT_EQ(1'b0, value[0]);

    model.stuck_wip = 1'b1;
    expect_err(FLASH_OP_ERASE, 24'hB00000, 24'h000001, FLASH_ERR_TIMEOUT);
    model.stuck_wip = 1'b0;
    model.wip = 1'b0;
    run_cmd(FLASH_OP_READ_ID, 24'd0, 24'd0, err, result);
    `ASSERT_EQ(FLASH_ERR_NONE, err);
    `ASSERT_EQ(32'h0020BA18, result);

    for (int i = 0; i < 300; i++) begin
      `ASSERT_EQ(golden[i], model.peek(32'h000100 + i));
    end
    `ASSERT_EQ(0, model.violations);

    `ASSERT_EQ(0, icap_words.size());
    run_cmd(FLASH_OP_REBOOT, 24'd0, 24'd0, err, result);
    `ASSERT_EQ(FLASH_ERR_NONE, err);
    repeat (32) @(posedge CLK);
    expected_icap = {32'hFFFFFFFF, 32'hAA995566, 32'h20000000, 32'h30020001, 32'h00000000, 32'h30008001, 32'h0000000F, 32'h20000000};
    `ASSERT_EQ(expected_icap.size(), icap_words.size());
    for (int i = 0; i < expected_icap.size(); i++) begin
      `ASSERT_EQ(expected_icap[i], icap_words[i]);
    end

    $display("OK! sim_flash");
    $finish();
  end

endmodule
