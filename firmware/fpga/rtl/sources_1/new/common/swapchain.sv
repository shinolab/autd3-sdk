`timescale 1ns / 1ps
`default_nettype none
module swapchain (
    input wire CLK,
    input wire [56:0] SYS_TIME,
    input wire UPDATE_SETTINGS,
    input wire REQ_RD_BANK,
    input wire [7:0] TRANSITION_MODE,
    input wire [63:0] TRANSITION_VALUE,
    input wire [15:0] CYCLE[params::NumBanks],
    input wire [15:0] REP[params::NumBanks],
    input wire [15:0] SYNC_IDX[params::NumBanks],
    input wire GPIO_IN[4],
    output wire STOP,
    output wire TRANSITION_PENDING,
    output wire EXT_ACTIVE,
    output wire BANK,
    output wire [15:0] IDX[params::NumBanks]
);

  localparam int Latency = 5;

  logic bank = 1'b0;
  logic req_bank;
  logic stop = 1'b0;
  logic ext_mode = 1'b0;
  logic [15:0] rep;
  logic [15:0] loop_cnt;

  logic idx_changed[params::NumBanks];
  logic [15:0] idx_old[params::NumBanks];
  logic [15:0] tic_idx[params::NumBanks];

  logic signed [57:0] time_diff;

  typedef enum logic [1:0] {
    WAIT_START,
    FINITE_LOOP,
    INFINITE_LOOP
  } state_t;

  state_t state = INFINITE_LOOP;

  logic [7:0] transition_mode;
  logic [1:0] gpio_pin;
  logic target_bank;
  logic transition_fires;
  logic cycle_end;
  logic tic_mode = 1'b0;

  assign BANK = bank;
  assign STOP = stop;
  assign TRANSITION_PENDING = state == WAIT_START;
  assign EXT_ACTIVE = ext_mode & (state == INFINITE_LOOP);
  assign target_bank = (state == WAIT_START) ? req_bank : bank;
  assign cycle_end = idx_changed[bank] && (tic_mode ? (tic_idx[bank] == CYCLE[bank]) : (SYNC_IDX[bank] == '0));

  logic [$clog2(Latency+1)-1:0] addsub_latency;
  logic [56:0] transition_time;
  sub57_57 addsub_diff_time (
      .CLK(CLK),
      .A  (SYS_TIME),
      .B  (transition_time),
      .S  (time_diff)
  );

  always_comb begin
    case (transition_mode)
      params::TRANSITION_MODE_SYNC_IDX: transition_fires = idx_changed[req_bank] && (SYNC_IDX[req_bank] == '0);
      params::TRANSITION_MODE_SYS_TIME: transition_fires = (addsub_latency == Latency) && (time_diff >= 58'sd0);
      params::TRANSITION_MODE_GPIO: transition_fires = idx_changed[req_bank] && GPIO_IN[gpio_pin];
      default: transition_fires = 1'b0;
    endcase
  end

  for (genvar i = 0; i < params::NumBanks; i++) begin : gen_swapchain
    always_ff @(posedge CLK) idx_old[i] <= SYNC_IDX[i];
    assign idx_changed[i] = idx_old[i] != SYNC_IDX[i];
    assign IDX[i] = tic_mode ? tic_idx[i] : idx_old[i];
  end

  always_ff @(posedge CLK) begin
    if (UPDATE_SETTINGS && (REP[REQ_RD_BANK] == params::RepInfinite)) begin
      stop <= 1'b0;
      bank <= REQ_RD_BANK;
      tic_mode <= 1'b0;
      ext_mode <= TRANSITION_MODE == params::TRANSITION_MODE_EXT;
      state <= INFINITE_LOOP;
    end else if (UPDATE_SETTINGS && (REQ_RD_BANK != target_bank)) begin
      rep <= REP[REQ_RD_BANK];
      req_bank <= REQ_RD_BANK;
      transition_mode <= TRANSITION_MODE;
      gpio_pin <= TRANSITION_VALUE[1:0];
      transition_time <= TRANSITION_VALUE[56:0];
      addsub_latency <= '0;
      state <= WAIT_START;
    end else begin
      case (state)
        WAIT_START: begin
          if (addsub_latency != Latency) addsub_latency <= addsub_latency + 1;
          if (transition_fires) begin
            stop <= 1'b0;
            loop_cnt <= '0;
            bank <= req_bank;
            tic_mode <= transition_mode != params::TRANSITION_MODE_SYNC_IDX;
            tic_idx[req_bank] <= '0;
            state <= FINITE_LOOP;
          end
        end
        INFINITE_LOOP: begin
          if (ext_mode && cycle_end) bank <= ~bank;
        end
        FINITE_LOOP: begin
          if (tic_mode && idx_changed[bank]) tic_idx[bank] <= cycle_end ? '0 : tic_idx[bank] + 1;
          if (cycle_end) begin
            if (loop_cnt == rep) stop <= 1'b1;
            else loop_cnt <= loop_cnt + 1;
          end
        end
        default: state <= INFINITE_LOOP;
      endcase
    end
  end

endmodule
`default_nettype wire
