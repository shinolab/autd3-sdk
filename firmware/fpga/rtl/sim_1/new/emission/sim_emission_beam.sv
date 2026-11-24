`timescale 1ns / 1ps
module sim_emission_beam ();

  `include "define.vh"

  logic CLK;
  logic locked;
  logic [56:0] SYS_TIME;
  logic UPDATE;

  localparam int DEPTH = 249;
  localparam int SIZE = 16;
  localparam int NumFixed = 4;
  localparam int MaxFoci = 3;

  sim_helper_bram sim_helper_bram ();
  sim_helper_random sim_helper_random ();

  settings::pattern_settings_t pattern_settings;

  int num_foci[params::NumBanks];
  logic [1:0] kind[params::NumBanks][SIZE*MaxFoci];
  logic signed [17:0] focus_x[params::NumBanks][SIZE*MaxFoci];
  logic signed [17:0] focus_y[params::NumBanks][SIZE*MaxFoci];
  logic signed [17:0] focus_z[params::NumBanks][SIZE*MaxFoci];
  logic [7:0] byte_buf[params::NumBanks][SIZE*MaxFoci];
  logic [31:0] checksum_expect[params::NumBanks][NumFixed];

  logic [15:0] debug_idx;
  logic debug_bank;
  logic [7:0] intensity;
  logic [7:0] phase;
  logic dout_valid;

  cnt_bus_if cnt_bus ();
  phase_corr_bus_if phase_corr_bus ();
  modulation_bus_if mod_bus ();
  emission_bus_if emission_bus ();
  pwe_table_bus_if pwe_table_bus ();
  flash_bus_if flash_bus ();
  output_mask_bus_if output_mask_bus ();

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

  sim_helper_clk sim_helper_clk (
      .CLK(CLK),
      .LOCKED(locked),
      .SYS_TIME(SYS_TIME)
  );

  time_cnt_generator time_cnt_generator (
      .CLK(CLK),
      .SYS_TIME(SYS_TIME),
      .SKIP_ONE_ASSERT(1'b0),
      .TIME_CNT(),
      .UPDATE(UPDATE)
  );

  emission #(
      .DEPTH(DEPTH)
  ) emission (
      .CLK(CLK),
      .SYS_TIME(SYS_TIME),
      .UPDATE(UPDATE),
      .PATTERN_SETTINGS(pattern_settings),
      .EMISSION_BUS(emission_bus.emission_port),
      .EMISSION_BUS_FOCUS(emission_bus.out_focus_port),
      .EMISSION_BUS_RAW(emission_bus.out_raw_port),
      .OUTPUT_MASK_BUS(output_mask_bus.out_port),
      .FAILSAFE(1'b0),
      .INTENSITY(intensity),
      .PHASE(phase),
      .DOUT_VALID(dout_valid),
      .DEBUG_IDX(debug_idx),
      .DEBUG_BANK(debug_bank)
  );

  int   cyc = 0;
  int   start_cyc = 0;
  int   first_latency = -1;
  int   last_latency = -1;
  logic dout_valid_d = 1'b0;
  always @(posedge CLK) begin
    cyc <= cyc + 1;
    dout_valid_d <= dout_valid;
    if (emission.start) start_cyc <= cyc;
    if (dout_valid & ~dout_valid_d) first_latency <= cyc - start_cyc;
    if (~dout_valid & dout_valid_d) last_latency <= cyc - start_cyc;
  end

  task automatic update(input logic req_bank, input logic [31:0] rep);
    @(posedge CLK);
    pattern_settings.UPDATE <= 1'b1;
    pattern_settings.REQ_RD_BANK <= req_bank;
    pattern_settings.REP[req_bank] <= rep;
    pattern_settings.CYCLE[req_bank] = SIZE - 1;
    pattern_settings.FREQ_DIV[req_bank] = 1;
    @(posedge CLK);
    pattern_settings.UPDATE <= 1'b0;
  endtask

  task automatic wait_bank(input logic bank);
    while (1) begin
      @(posedge CLK);
      if (debug_bank === bank) begin
        break;
      end
    end
  endtask

  logic [7:0] sin_table [  256];
  logic [7:0] atan_table[16384];

  function automatic logic [6:0] atan_axis(input longint v, input int bits);
    automatic longint q;
    if (bits > 6) begin
      q = (v >>> (bits - 6)) + ((v >>> (bits - 7)) & 1);
      if (q == 64) q = 63;
    end else begin
      q = v * (1 << (6 - bits));
    end
    return 7'(q + 64);
  endfunction

  function automatic logic [7:0] additive_phase(input logic [1:0] t, input logic signed [17:0] x, input logic signed [17:0] y,
                                                input logic signed [17:0] z, input int tx, input int ty);
    automatic longint dx, dy, adx, ady, m;
    automatic logic signed [63:0] acc;
    automatic logic [13:0] key;
    automatic int bits;
    if (t == 2'b10) begin
      acc = longint'(x) * tx + longint'(y) * ty + longint'(z) * 32;
      return acc[22:15];
    end
    dx  = tx - x;
    dy  = ty - y;
    adx = dx < 0 ? -dx : dx;
    ady = dy < 0 ? -dy : dy;
    m   = adx > ady ? adx : ady;
    if (m == 0) begin
      key = {7'd64, 7'd64};
    end else begin
      bits = $clog2(m + 1);
      key  = {atan_axis(dy, bits), atan_axis(dx, bits)};
    end
    return 8'(z[15:8] - z[7:0] * atan_table[key]);
  endfunction

  function automatic logic [7:0] expected_phase(input int bank, input int sample, input int tx, input int ty);
    automatic int n = num_foci[bank];
    automatic int sin_acc = 0, cos_acc = 0, count = 0;
    automatic logic [7:0] base, added = 8'd0, sin_ave, cos_ave;
    for (int k = 0; k < n; k++) begin
      automatic int w = sample * n + k;
      automatic logic [1:0] t = kind[bank][w];
      if (t[1] == 1'b0) begin
        automatic longint dx = focus_x[bank][w] - tx;
        automatic longint dy = focus_y[bank][w] - ty;
        automatic longint dz = (t == 2'b01) ? 0 : focus_z[bank][w];
        automatic longint r = $rtoi($sqrt($itor(dx * dx + dy * dy + dz * dz)));
        automatic longint divisor = (t == 2'b01) ? longint'(focus_z[bank][w][15:0]) : longint'(pattern_settings.SOUND_SPEED[bank]);
        automatic logic [7:0] p = 8'((r << 14) / divisor);
        if (k != 0) p = p + byte_buf[bank][w];
        sin_acc += sin_table[p];
        cos_acc += sin_table[8'(p+64)];
        count++;
      end else begin
        added = additive_phase(t, focus_x[bank][w], focus_y[bank][w], focus_z[bank][w], tx, ty);
      end
    end
    if (count == 0) begin
      base = 8'd0;
    end else begin
      sin_ave = 8'(sin_acc / count);
      cos_ave = 8'(cos_acc / count);
      base = atan_table[{sin_ave[7:1], cos_ave[7:1]}];
    end
    return 8'(base + added);
  endfunction

  task automatic check(input logic bank);
    automatic int idx, ix, iy, tx, ty;
    automatic int debug_idx_buf;
    automatic logic [31:0] checksum;

    while (1) begin
      @(posedge CLK);
      if (~dout_valid) begin
        break;
      end
    end
    for (int j = 0; j < SIZE; j++) begin
      while (1) begin
        @(posedge CLK);
        if (dout_valid) begin
          break;
        end
      end
      $display("check bank %d sample %d @%d", bank, debug_idx, SYS_TIME);
      idx = 0;
      checksum = 0;
      debug_idx_buf = debug_idx;
      for (int id = 0; idx < DEPTH; id++) begin
        ix = id % 18;
        iy = id / 18;
        if ((iy === 1) && (ix === 1 || ix === 2 || ix === 16)) begin
          continue;
        end
        tx = int'(10.16 * ix / 0.025);
        ty = int'(10.16 * iy / 0.025);
        `ASSERT_EQ(byte_buf[bank][debug_idx_buf*num_foci[bank]], intensity);
        `ASSERT_EQ(expected_phase(bank, debug_idx_buf, tx, ty), phase);
        checksum = checksum + phase * (idx + 1);
        @(posedge CLK);
        idx++;
      end
      if (debug_idx_buf < NumFixed) begin
        $display("checksum bank %0d sample %0d = 0x%08h", bank, debug_idx_buf, checksum);
        `ASSERT_EQ(checksum_expect[bank][debug_idx_buf], checksum);
      end
    end
  endtask

  task automatic set_word(input int bank, input int w, input logic [1:0] t, input int x, input int y, input int z, input logic [7:0] b);
    kind[bank][w] = t;
    focus_x[bank][w] = 18'(x);
    focus_y[bank][w] = 18'(y);
    focus_z[bank][w] = 18'(z);
    byte_buf[bank][w] = b;
  endtask

  task automatic set_random_word(input int bank, input int w, input logic [1:0] t);
    automatic logic signed [17:0] x, y, z;
    if (t == 2'b10) begin
      x = sim_helper_random.range(131071, -131072);
      y = sim_helper_random.range(131071, -131072);
      z = sim_helper_random.range(131071, -131072);
    end else begin
      x = sim_helper_random.range(131071, -131072 + 6909);
      y = sim_helper_random.range(131071, -131072 + 5283);
      case (t)
        2'b00:   z = sim_helper_random.range(131071, -131072);
        2'b01:   z = sim_helper_random.range(65535, 1);
        default: z = sim_helper_random.range(65535, 0);
      endcase
    end
    set_word(bank, w, t, x, y, z, sim_helper_random.range(8'hFF, 0));
  endtask

  initial begin
    $readmemh("sin.txt", sin_table);
    $readmemh("atan.txt", atan_table);

    num_foci[0] = MaxFoci;
    num_foci[1] = 1;

    pattern_settings.UPDATE = 0;
    pattern_settings.TRANSITION_MODE = params::TRANSITION_MODE_SYNC_IDX;
    pattern_settings.TRANSITION_VALUE = 0;
    pattern_settings.MODE[0] = params::EMISSION_TYPE_FOCI;
    pattern_settings.MODE[1] = params::EMISSION_TYPE_FOCI;
    pattern_settings.SOUND_SPEED[0] = 340 * 64;
    pattern_settings.SOUND_SPEED[1] = 340 * 64;
    pattern_settings.CYCLE[0] = '0;
    pattern_settings.FREQ_DIV[0] = '1;
    pattern_settings.NUM_FOCI[0] = MaxFoci;
    pattern_settings.CYCLE[1] = '0;
    pattern_settings.FREQ_DIV[1] = '1;
    pattern_settings.NUM_FOCI[1] = 1;

    set_word(0, 0, 2'b00, 1000, 2000, 6000, 8'hAA);
    set_word(0, 1, 2'b11, 3000, 2500, 16'h4002, 8'h00);
    set_word(0, 2, 2'b00, -4000, 3000, 8000, 8'h30);
    set_word(0, 3, 2'b01, 3000, 2600, 23000, 8'h80);
    set_word(0, 4, 2'b01, -2000, 100, 40000, 8'h10);
    set_word(0, 5, 2'b00, 500, 500, 4000, 8'h55);
    set_word(0, 6, 2'b10, 20000, -15000, -30000, 8'h7F);
    set_word(0, 7, 2'b00, 2000, 2000, 5000, 8'h11);
    set_word(0, 8, 2'b01, 1000, 1000, 30000, 8'h22);
    set_word(0, 9, 2'b11, -500, 7000, 16'hC3FD, 8'hFF);
    set_word(0, 10, 2'b00, 3500, 2600, 6000, 8'h00);
    set_word(0, 11, 2'b01, 3456, 2640, 21760, 8'hC0);
    set_word(1, 0, 2'b10, -24000, 6000, 77777, 8'h12);
    set_word(1, 1, 2'b11, 3456, 2640, 16'h0001, 8'h34);
    set_word(1, 2, 2'b01, 3456, 2640, 25000, 8'h56);
    set_word(1, 3, 2'b00, 3456, 2640, 6000, 8'h78);

    checksum_expect[0][0] = 32'h003cc3a0;
    checksum_expect[0][1] = 32'h003aaee7;
    checksum_expect[0][2] = 32'h003abdad;
    checksum_expect[0][3] = 32'h003e6aef;
    checksum_expect[1][0] = 32'h003c626c;
    checksum_expect[1][1] = 32'h002d4a45;
    checksum_expect[1][2] = 32'h00324e40;
    checksum_expect[1][3] = 32'h0039f92d;

    for (int i = NumFixed; i < SIZE; i++) begin
      automatic int additive_at = sim_helper_random.range(MaxFoci, 0);
      for (int k = 0; k < MaxFoci; k++) begin
        set_random_word(0, i * MaxFoci + k, k == additive_at ? 2'(2 + sim_helper_random.range(1, 0)) : 2'(sim_helper_random.range(1, 0)));
      end
      set_random_word(1, i, 2'(sim_helper_random.range(3, 0)));
    end

    @(posedge locked);

    for (int bank = 0; bank < params::NumBanks; bank++) begin
      sim_helper_bram.write_emission_beam(bank, kind[bank], focus_x[bank], focus_y[bank], focus_z[bank], byte_buf[bank], SIZE * num_foci[bank]);
    end
    $display("memory initialized");

    fork
      update(0, 32'hFFFFFFFF);
      wait_bank(0);
    join
    check(0);

    fork
      update(1, 32'd0);
      wait_bank(1);
    join
    check(1);

    $display("cycles from START: first output %0d, last output %0d", first_latency, last_latency);
    $display("OK! sim_emission_beam");
    $finish();
  end

endmodule
