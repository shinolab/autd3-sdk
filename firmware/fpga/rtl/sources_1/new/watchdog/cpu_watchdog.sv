`timescale 1ns / 1ps
`default_nettype none
module cpu_watchdog #(
    parameter int CYCLES_PER_MS = params::SyncCycleTicks
) (
    input wire BUS_CLK,
    input wire CLK,
    input wire EN,
    input wire WE,
    input wire [7:0] ADDR,
    input wire [15:0] DATA_IN,
    output var [15:0] DATA_OUT,
    output var FIRED
);

  import params::*;

  logic [15:0] timeout_reg = 16'd0;
  logic kick_req = 1'b0;
  logic timeout_req = 1'b0;
  logic release_req = 1'b0;
  logic [15:0] rdata = 16'd0;

  logic fired = 1'b0;
  logic [7:0] count = 8'd0;
  logic [7:0] count_gray = 8'd0;

  logic fired_sync;
  logic [7:0] count_gray_sync;

  cdc_sync fired_cdc (
      .CLK(BUS_CLK),
      .D  (fired),
      .Q  (fired_sync)
  );

  cdc_sync #(
      .WIDTH(8)
  ) count_gray_cdc (
      .CLK(BUS_CLK),
      .D  (count_gray),
      .Q  (count_gray_sync)
  );

  function automatic logic [7:0] gray_to_bin(input logic [7:0] gray);
    logic [7:0] bin;
    bin[7] = gray[7];
    for (int i = 6; i >= 0; i--) begin
      bin[i] = bin[i+1] ^ gray[i];
    end
    return bin;
  endfunction

  assign DATA_OUT = rdata;
  assign FIRED = fired;

  always_ff @(posedge BUS_CLK) begin
    if (WE & EN) begin
      case (ADDR)
        ADDR_WATCHDOG_KICK: kick_req <= ~kick_req;
        ADDR_WATCHDOG_TIMEOUT: begin
          timeout_reg <= DATA_IN;
          timeout_req <= ~timeout_req;
        end
        ADDR_WATCHDOG_RELEASE: release_req <= ~release_req;
        default: begin
        end
      endcase
    end
    if (EN) begin
      case (ADDR)
        ADDR_WATCHDOG_TIMEOUT: rdata <= timeout_reg;
        ADDR_WATCHDOG_STATE: rdata <= {gray_to_bin(count_gray_sync), 7'd0, fired_sync};
        default: rdata <= 16'd0;
      endcase
    end
  end

  logic kicked;
  logic timeout_written;
  logic released;

  cdc_toggle kick_cdc (
      .CLK   (CLK),
      .TOGGLE(kick_req),
      .PULSE (kicked)
  );

  cdc_toggle timeout_cdc (
      .CLK   (CLK),
      .TOGGLE(timeout_req),
      .PULSE (timeout_written)
  );

  cdc_toggle release_cdc (
      .CLK   (CLK),
      .TOGGLE(release_req),
      .PULSE (released)
  );

  logic [15:0] timeout = 16'd0;
  logic [$clog2(CYCLES_PER_MS)-1:0] prescaler = '0;
  logic [15:0] elapsed_ms = 16'd0;

  always_ff @(posedge CLK) begin
    if (timeout_written) begin
      timeout <= timeout_reg;
    end
    if (released) begin
      fired <= 1'b0;
    end

    if (kicked | timeout_written | released | fired | (timeout == 16'd0)) begin
      prescaler  <= '0;
      elapsed_ms <= 16'd0;
    end else begin
      if (prescaler == CYCLES_PER_MS - 1) begin
        prescaler  <= '0;
        elapsed_ms <= elapsed_ms + 16'd1;
      end else begin
        prescaler <= prescaler + 1'b1;
      end
      if (elapsed_ms >= timeout) begin
        fired <= 1'b1;
        if (count != 8'hFF) begin
          count <= count + 8'd1;
        end
      end
    end

    count_gray <= count ^ {1'b0, count[7:1]};
  end

endmodule
`default_nettype wire
