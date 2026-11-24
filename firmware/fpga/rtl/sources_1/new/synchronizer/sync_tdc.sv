`timescale 1ns / 1ps
`default_nettype none
module sync_tdc (
    input wire CLK,
    input wire BUS_CLK,
    input wire RESET,
    input wire SYNC_IN,
    input wire settings::sync_tdc_ctl_t CTL,
    output wire TDC_CLK,
    output var settings::sync_tdc_status_t STATUS
);

  localparam int Carry4s = 128;
  localparam int Taps = Carry4s * 4;
  localparam int Words = Taps / 16;
  localparam int Entries = 128;
  localparam int SettleCycles = 8;

  logic pll_fb;
  logic pll_fb_buf;
  logic pll_out;
  logic tdc_clk;
  logic locked;

  PLLE2_BASE #(
      .CLKIN1_PERIOD (48.828),
      .CLKFBOUT_MULT (50),
      .DIVCLK_DIVIDE (1),
      .CLKOUT0_DIVIDE(5)
  ) pll (
      .CLKIN1(CLK),
      .CLKFBIN(pll_fb_buf),
      .CLKFBOUT(pll_fb),
      .CLKOUT0(pll_out),
      .CLKOUT1(),
      .CLKOUT2(),
      .CLKOUT3(),
      .CLKOUT4(),
      .CLKOUT5(),
      .LOCKED(locked),
      .PWRDWN(1'b0),
      .RST(RESET)
  );

  BUFG pll_fb_bufg (
      .I(pll_fb),
      .O(pll_fb_buf)
  );

  BUFG tdc_clk_bufg (
      .I(pll_out),
      .O(tdc_clk)
  );

  assign TDC_CLK = tdc_clk;

  logic calib = 1'b0;
  always_ff @(posedge BUS_CLK) calib <= ~calib;

  logic source_clk = 1'b0;
  logic [7:0] start_count = '0;
  logic [7:0] done_count = '0;
  logic [7:0] done_count_seen = '0;
  logic clk_phase = 1'b0;

  always_ff @(posedge CLK) begin
    clk_phase <= ~clk_phase;
    done_count_seen <= done_count;
    if (CTL.START) begin
      source_clk  <= CTL.SOURCE;
      start_count <= start_count + 8'd1;
    end
  end

  logic source = 1'b0;
  logic chain_in;
  assign chain_in = source ? calib : SYNC_IN;

  logic [Taps-1:0] taps;
  for (genvar i = 0; i < Carry4s; i++) begin : gen_chain
    if (i == 0) begin : gen_head
      (* DONT_TOUCH = "TRUE" *)
      CARRY4 carry4 (
          .CO(taps[3:0]),
          .O(),
          .CI(1'b0),
          .CYINIT(chain_in),
          .DI(4'b0000),
          .S(4'b1111)
      );
    end else begin : gen_link
      (* DONT_TOUCH = "TRUE" *)
      CARRY4 carry4 (
          .CO(taps[4*i+:4]),
          .O(),
          .CI(taps[4*i-1]),
          .CYINIT(1'b0),
          .DI(4'b0000),
          .S(4'b1111)
      );
    end
  end

  (* DONT_TOUCH = "TRUE" *) logic [Taps-1:0] taps_meta = '0;
  logic [Taps-1:0] taps_q = '0;
  logic tap_head_prev = 1'b0;
  logic hit;
  assign hit = taps_q[0] & ~tap_head_prev;

  logic clk_phase_seen = 1'b0;
  logic clk_phase_prev = 1'b0;
  logic [3:0] slot = '0;
  logic [7:0] start_seen = '0;
  logic [7:0] start_prev = '0;
  logic locked_tdc;

  cdc_sync locked_cdc (
      .CLK(tdc_clk),
      .D  (locked),
      .Q  (locked_tdc)
  );

  typedef enum logic [2:0] {
    IDLE,
    SETTLE,
    WAIT_HIT,
    COUNT,
    DRAIN,
    STORE
  } state_t;
  state_t state = IDLE;

  logic [Taps-1:0] held = '0;
  logic [3:0] held_slot = '0;
  logic [4:0] word = '0;
  logic [4:0] word_ones = '0;
  logic [9:0] ones = '0;
  logic [6:0] entry = '0;
  logic we = 1'b0;
  logic [6:0] waddr = '0;
  logic [15:0] wdata = '0;

  assign STATUS.CAPTURING = start_count != done_count_seen;
  assign STATUS.LOCKED = locked;
  assign STATUS.WE = we;
  assign STATUS.ADDR = waddr;
  assign STATUS.DATA = wdata;

  function automatic logic [4:0] popcount16(input logic [15:0] v);
    logic [4:0] n;
    n = '0;
    for (int i = 0; i < 16; i++) n = n + 5'(v[i]);
    return n;
  endfunction

  always_ff @(posedge tdc_clk) begin
    taps_meta <= taps;
    taps_q <= taps_meta;
    tap_head_prev <= taps_q[0];
    clk_phase_seen <= clk_phase;
    clk_phase_prev <= clk_phase_seen;
    slot <= (clk_phase_seen != clk_phase_prev) ? 4'd0 : slot + 4'd1;
    start_seen <= start_count;
    start_prev <= start_seen;
    word_ones <= popcount16(held[15:0]);
    we <= 1'b0;
    if (!locked_tdc) begin
      source <= 1'b0;
      state  <= IDLE;
    end else if (start_seen != start_prev) begin
      source <= source_clk;
      entry  <= '0;
      word   <= '0;
      state  <= SETTLE;
    end else begin
      case (state)
        IDLE: begin
        end
        SETTLE: begin
          word <= word + 5'd1;
          if (word == 5'(SettleCycles - 1)) state <= WAIT_HIT;
        end
        WAIT_HIT: begin
          if (hit) begin
            held <= taps_q;
            held_slot <= slot;
            ones <= '0;
            word <= '0;
            state <= COUNT;
          end
        end
        COUNT: begin
          held <= {16'd0, held[Taps-1:16]};
          if (word != '0) ones <= ones + 10'(word_ones);
          word <= word + 5'd1;
          if (word == 5'(Words - 1)) state <= DRAIN;
        end
        DRAIN: begin
          ones  <= ones + 10'(word_ones);
          state <= STORE;
        end
        STORE: begin
          we <= 1'b1;
          waddr <= entry;
          wdata <= {2'b00, held_slot, ones};
          entry <= entry + 7'd1;
          if (entry == 7'(Entries - 1)) begin
            done_count <= start_seen;
            source <= 1'b0;
            state <= IDLE;
          end else begin
            state <= WAIT_HIT;
          end
        end
        default: state <= IDLE;
      endcase
    end
  end

endmodule
`default_nettype wire
