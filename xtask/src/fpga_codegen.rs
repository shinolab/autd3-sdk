use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::util::on_path;

const BRAM_DATA_WIDTH: u32 = 16;

const FIXED_STATES: [&str; 8] = [
    "REQ_WR_VER_PATCH",
    "REQ_WR_VER_MINOR",
    "REQ_WR_VER_MAJOR",
    "REQ_WR_FUNCTION_BITS",
    "WR_FPGA_STATE_RD_CTL_FLAG",
    "REQ_RD_CTL_FLAG",
    "SET_DONE_0",
    "SET_DONE_1",
];

#[derive(Clone)]
struct Param {
    addr: String,
    width: u32,
    name: String,
    default: &'static str,
}

impl Param {
    fn new(addr: &str, width: u32, name: &str, default: &'static str) -> Self {
        Self {
            addr: addr.to_string(),
            width,
            name: name.to_string(),
            default,
        }
    }
}

struct Group {
    name: &'static str,
    params: Vec<Param>,
    reject: Option<&'static str>,
    transition_invalid: Option<&'static str>,
}

impl Group {
    fn target(&self) -> String {
        if self.reject.is_some() {
            format!("{}_stage", self.name.to_lowercase())
        } else {
            format!("{}_SETTINGS", self.name)
        }
    }
}

struct State {
    name: String,
    req: Option<Param>,
    param: Option<Param>,
}

#[allow(clippy::too_many_lines)]
pub(crate) fn gen_controller(fpga_dir: &Path, check: bool) -> Result<()> {
    let path = fpga_dir.join("rtl/sources_1/new/controller/controller.sv");
    if !on_path("verible-verilog-format") {
        bail!(
            "verible-verilog-format not found on PATH. Install Verible so the generated \
         controller.sv can be formatted."
        );
    }
    let mut child = Command::new("verible-verilog-format")
        .args([crate::fpga::VERIBLE_COLUMN_LIMIT, "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("failed to spawn verible-verilog-format")?;
    let mut stdin = child
        .stdin
        .take()
        .context("verible-verilog-format has no stdin")?;
    let debug_default = "{params::GPIO_O_TYPE_NONE, 56'd0}";
    let groups = vec![
        Group {
            name: "MOD",
            reject: Some("mod_reject"),
            transition_invalid: Some("mod_transition_invalid"),
            params: vec![
                Param::new("MOD_REQ_RD_BANK", 1, "REQ_RD_BANK", "1'd0"),
                Param::new(
                    "MOD_TRANSITION_MODE",
                    8,
                    "TRANSITION_MODE",
                    "params::TRANSITION_MODE_SYNC_IDX",
                ),
                Param::new("MOD_TRANSITION_VALUE", 64, "TRANSITION_VALUE", "64'd0"),
                Param::new("MOD_CYCLE0", 16, "CYCLE[0]", "16'd1"),
                Param::new("MOD_CYCLE1", 16, "CYCLE[1]", "16'd1"),
                Param::new("MOD_FREQ_DIV0", 16, "FREQ_DIV[0]", "16'd10"),
                Param::new("MOD_FREQ_DIV1", 16, "FREQ_DIV[1]", "16'd10"),
                Param::new("MOD_REP0", 16, "REP[0]", "16'hFFFF"),
                Param::new("MOD_REP1", 16, "REP[1]", "16'hFFFF"),
            ],
        },
        Group {
            name: "PATTERN",
            reject: Some("pattern_reject"),
            transition_invalid: Some("pattern_transition_invalid"),
            params: vec![
                Param::new("PATTERN_REQ_RD_BANK", 1, "REQ_RD_BANK", "1'd0"),
                Param::new(
                    "PATTERN_TRANSITION_MODE",
                    8,
                    "TRANSITION_MODE",
                    "params::TRANSITION_MODE_SYNC_IDX",
                ),
                Param::new("PATTERN_TRANSITION_VALUE", 64, "TRANSITION_VALUE", "64'd0"),
                Param::new("PATTERN_MODE0", 1, "MODE[0]", "params::EMISSION_TYPE_RAW"),
                Param::new("PATTERN_MODE1", 1, "MODE[1]", "params::EMISSION_TYPE_RAW"),
                Param::new("PATTERN_CYCLE0", 16, "CYCLE[0]", "16'd0"),
                Param::new("PATTERN_CYCLE1", 16, "CYCLE[1]", "16'd0"),
                Param::new("PATTERN_FREQ_DIV0", 16, "FREQ_DIV[0]", "16'hFFFF"),
                Param::new("PATTERN_FREQ_DIV1", 16, "FREQ_DIV[1]", "16'hFFFF"),
                Param::new("PATTERN_SOUND_SPEED0", 16, "SOUND_SPEED[0]", "16'd0"),
                Param::new("PATTERN_SOUND_SPEED1", 16, "SOUND_SPEED[1]", "16'd0"),
                Param::new("PATTERN_REP0", 16, "REP[0]", "16'hFFFF"),
                Param::new("PATTERN_REP1", 16, "REP[1]", "16'hFFFF"),
                Param::new("PATTERN_NUM_FOCI0", 8, "NUM_FOCI[0]", "1"),
                Param::new("PATTERN_NUM_FOCI1", 8, "NUM_FOCI[1]", "1"),
            ],
        },
        Group {
            name: "SILENCER",
            reject: Some("silencer_reject"),
            transition_invalid: None,
            params: vec![
                Param::new("SILENCER_FLAG", 8, "FLAG", "8'd0"),
                Param::new(
                    "SILENCER_UPDATE_RATE_INTENSITY",
                    16,
                    "UPDATE_RATE_INTENSITY",
                    "16'd256",
                ),
                Param::new(
                    "SILENCER_UPDATE_RATE_PHASE",
                    16,
                    "UPDATE_RATE_PHASE",
                    "16'd256",
                ),
                Param::new(
                    "SILENCER_COMPLETION_STEPS_INTENSITY",
                    16,
                    "COMPLETION_STEPS_INTENSITY",
                    "16'd10",
                ),
                Param::new(
                    "SILENCER_COMPLETION_STEPS_PHASE",
                    16,
                    "COMPLETION_STEPS_PHASE",
                    "16'd40",
                ),
            ],
        },
        Group {
            name: "DEBUG",
            reject: None,
            transition_invalid: None,
            params: vec![
                Param::new("DEBUG_VALUE0", 64, "VALUE[0]", debug_default),
                Param::new("DEBUG_VALUE1", 64, "VALUE[1]", debug_default),
                Param::new("DEBUG_VALUE2", 64, "VALUE[2]", debug_default),
                Param::new("DEBUG_VALUE3", 64, "VALUE[3]", debug_default),
            ],
        },
        Group {
            name: "SYNC",
            reject: None,
            transition_invalid: None,
            params: vec![Param::new("SYNC_TIME", 64, "SYNC_TIME", "64'd0")],
        },
    ];
    let mut all_states: Vec<(&Group, Vec<State>)> = Vec::new();
    for group in &groups {
        let mut sub = Vec::new();
        for param in &group.params {
            if param.width <= BRAM_DATA_WIDTH {
                sub.push(param.clone());
            } else {
                let n = param.width / BRAM_DATA_WIDTH;
                for i in 0..n {
                    let hi = param.width.min(BRAM_DATA_WIDTH * (i + 1)) - 1;
                    let lo = BRAM_DATA_WIDTH * i;
                    sub.push(Param {
                        addr: format!("{}_{i}", param.addr),
                        width: BRAM_DATA_WIDTH.min(param.width - BRAM_DATA_WIDTH * i),
                        name: format!("{}[{hi}:{lo}]", param.name),
                        default: param.default,
                    });
                }
            }
        }
        let n = sub.len();
        let mut states = Vec::new();
        for k in 0..n + 3 {
            let req = sub.get(k).cloned();
            let param = k.checked_sub(3).map(|i| sub[i].clone());
            let name = req
                .iter()
                .map(|p| format!("REQ_{}", p.addr))
                .chain(param.iter().map(|p| format!("RD_{}", p.addr)))
                .collect::<Vec<_>>()
                .join("_");
            states.push(State { name, req, param });
        }
        let tails: &[&str] = if group.reject.is_some() {
            &["VALIDATE", "COMMIT", "CLR_UPDATE_SETTINGS_BIT"]
        } else {
            &["COMMIT", "CLR_UPDATE_SETTINGS_BIT"]
        };
        for tail in tails {
            states.push(State {
                name: format!("{}_{tail}", group.name),
                req: None,
                param: None,
            });
        }
        all_states.push((group, states));
    }
    let total: usize = all_states.iter().map(|(_, s)| s.len()).sum();
    let enum_width = (u32::try_from(FIXED_STATES.len() + total).unwrap() - 1).ilog2() + 1;
    let mut source = String::new();
    source.push_str(
        r"`timescale 1ns / 1ps
`default_nettype none
module controller (
    input wire CLK,
    input wire ENABLE,
    input wire THERMO,
    input wire PATTERN_BANK,
    input wire MOD_BANK,
    input wire PATTERN_EXT_ACTIVE,
    input wire MOD_EXT_ACTIVE,
    input wire [15:0] PATTERN_CYCLE,
    input wire PATTERN_STOPPED,
    input wire MOD_STOPPED,
    input wire PATTERN_TRANSITION_PENDING,
    input wire MOD_TRANSITION_PENDING,
    input wire [7:0] SYNC_RESYNC_COUNT,
    cnt_bus_if.out_port cnt_bus,
    output var settings::mod_settings_t MOD_SETTINGS,
    output var settings::pattern_settings_t PATTERN_SETTINGS,
    output var settings::silencer_settings_t SILENCER_SETTINGS,
    output var settings::sync_settings_t SYNC_SETTINGS,
    output var settings::debug_settings_t DEBUG_SETTINGS,
    output var FORCE_FAN,
    output var FAILSAFE,
    output var GPIO_IN[4]
);

  localparam bit [7:0] FunctionBits = (1'b0 << params::FuncDynamicFreqBit)
                                      | (1'b1 << params::FuncFlashOtaBit)
                                      | (1'b1 << params::FuncStrictSilencerGuardBit)
                                      | (1'b1 << params::FuncCpuWatchdogBit)
                                      | (1'b1 << params::FuncSyncPhaseLockBit)
                                      | (1'b0 << params::FuncEmulatorBit);

  logic [15:0] ctl_flags = '0;
  logic [15:0] ctl_flags_cand = '0;
  logic [15:0] set_result = '0;

  logic we = 1'b0;
  logic [7:0]  addr;
  logic [15:0] din;
  logic [15:0] dout;

  logic [15:0] fpga_state_prev = 16'hFFFF;

  assign cnt_bus.WE = we;
  assign cnt_bus.ADDR = addr;
  assign cnt_bus.DIN = din;
  assign dout = cnt_bus.DOUT;

  assign FORCE_FAN = ctl_flags[params::CTL_FLAG_BIT_FORCE_FAN];
  assign FAILSAFE = ctl_flags[params::CTL_FLAG_BIT_FAILSAFE];
  assign GPIO_IN[0] = ctl_flags[params::CTL_FLAG_BIT_GPIO_IN_0];
  assign GPIO_IN[1] = ctl_flags[params::CTL_FLAG_BIT_GPIO_IN_1];
  assign GPIO_IN[2] = ctl_flags[params::CTL_FLAG_BIT_GPIO_IN_2];
  assign GPIO_IN[3] = ctl_flags[params::CTL_FLAG_BIT_GPIO_IN_3];

  function automatic logic [15:0] fpga_state_din();
    logic [15:0] s = '0;
    s[params::FPGA_STATE_BIT_THERMAL_ASSERT] = THERMO;
    s[params::FPGA_STATE_BIT_MOD_BANK] = MOD_BANK;
    s[params::FPGA_STATE_BIT_PATTERN_BANK] = PATTERN_BANK;
    s[params::FPGA_STATE_BIT_PATTERN_MODE] = PATTERN_CYCLE == '0;
    s[params::FPGA_STATE_BIT_PATTERN_STOPPED] = PATTERN_STOPPED;
    s[params::FPGA_STATE_BIT_MOD_STOPPED] = MOD_STOPPED;
    s[params::FPGA_STATE_BIT_PATTERN_TRANSITION_PENDING] = PATTERN_TRANSITION_PENDING;
    s[params::FPGA_STATE_BIT_MOD_TRANSITION_PENDING] = MOD_TRANSITION_PENDING;
    s[15:8] = SYNC_RESYNC_COUNT;
    return s;
  endfunction

",
    );
    source.push_str(
        r"  settings::mod_settings_t mod_stage;
  settings::pattern_settings_t pattern_stage;
  settings::silencer_settings_t silencer_stage;

  function automatic logic is_strict(input logic [7:0] flag);
    return flag[params::SILENCER_FLAG_BIT_STRICT_MODE]
           & ~flag[params::SILENCER_FLAG_BIT_FIXED_UPDATE_RATE_MODE];
  endfunction

",
    );
    for (name, extra) in [("mod", false), ("pattern", true)] {
        let upper = name.to_uppercase();
        let violates = |div: &str, steps: &str| {
            if extra {
                format!(
                    r"(({div} < {steps}.COMPLETION_STEPS_INTENSITY)
           | ({div} < {steps}.COMPLETION_STEPS_PHASE))"
                )
            } else {
                format!("({div} < {steps}.COMPLETION_STEPS_INTENSITY)")
            }
        };
        writeln!(
                            &mut source,
                            r"  logic {name}_stage_finite;
  logic {name}_now_finite;
  logic {name}_stage_all;
  logic {name}_now_all;
  logic [params::NumBanks-1:0] {name}_stage_ng;
  logic [params::NumBanks-1:0] {name}_now_ng;
  logic {name}_transition_invalid;
  logic {name}_reject;
  assign {name}_stage_finite = {name}_stage.REP[{name}_stage.REQ_RD_BANK] != params::RepInfinite;
  assign {name}_now_finite = {upper}_SETTINGS.REP[{upper}_SETTINGS.REQ_RD_BANK] != params::RepInfinite;
  assign {name}_now_all = {name}_now_finite ? {upper}_EXT_ACTIVE
      : ({upper}_SETTINGS.TRANSITION_MODE == params::TRANSITION_MODE_EXT);
  assign {name}_stage_all = {name}_stage_finite ? {name}_now_all
      : ({name}_stage.TRANSITION_MODE == params::TRANSITION_MODE_EXT);
  for (genvar b = 0; b < params::NumBanks; b++) begin : gen_{name}_strict_guard
    logic stage_used;
    logic now_used;
    assign stage_used = {name}_stage_all | ({name}_stage.REQ_RD_BANK == 1'(b))
        | ({name}_stage_finite & ({upper}_BANK == 1'(b)));
    assign now_used = {name}_now_all | ({upper}_SETTINGS.REQ_RD_BANK == 1'(b))
        | ({name}_now_finite & ({upper}_BANK == 1'(b)));
    assign {name}_stage_ng[b] = stage_used
        & {};
    assign {name}_now_ng[b] = now_used
        & {};
  end
  assign {name}_transition_invalid = {name}_stage_finite
      ? !({name}_stage.TRANSITION_MODE inside {{params::TRANSITION_MODE_SYNC_IDX,
          params::TRANSITION_MODE_SYS_TIME, params::TRANSITION_MODE_GPIO}})
      : !({name}_stage.TRANSITION_MODE inside {{params::TRANSITION_MODE_IMMEDIATE,
          params::TRANSITION_MODE_EXT}});
  assign {name}_reject = (is_strict(SILENCER_SETTINGS.FLAG) & (|{name}_stage_ng))
      | {name}_transition_invalid;
",
                            violates(&format!("{name}_stage.FREQ_DIV[b]"), "SILENCER_SETTINGS"),
                            violates(&format!("{upper}_SETTINGS.FREQ_DIV[b]"), "silencer_stage"),
                        )
                        .unwrap();
    }
    source.push_str(
        r"  logic silencer_reject;
  assign silencer_reject = is_strict(silencer_stage.FLAG) & ((|mod_now_ng) | (|pattern_now_ng));

",
    );
    writeln!(source, "  typedef enum logic [{}:0] {{", enum_width - 1).unwrap();
    let enum_body: Vec<String> = FIXED_STATES
        .iter()
        .copied()
        .chain(
            all_states
                .iter()
                .flat_map(|(_, states)| states.iter())
                .map(|s| s.name.as_str()),
        )
        .map(|name| format!("    {name}"))
        .collect();
    source.push_str(&enum_body.join(",\n"));
    source.push_str("\n  } state_t;\n\n  state_t state = REQ_WR_VER_PATCH;\n");
    source.push_str(
        r"
  always_ff @(posedge CLK) begin
    if (!ENABLE) begin
      state <= REQ_WR_VER_PATCH;
      we <= 1'b0;
      fpga_state_prev <= 16'hFFFF;
      ctl_flags <= '0;
      ctl_flags_cand <= '0;
      set_result <= '0;
",
    );
    for (group, _) in &all_states {
        writeln!(&mut source, "      {}_SETTINGS.UPDATE <= 1'b0;", group.name).unwrap();
    }
    source.push_str(
        r"    end else case (state)
      REQ_WR_VER_PATCH: begin
        we <= 1'b1;

        din <= {8'd0, params::VersionNumPatch};
        addr <= params::ADDR_VERSION_NUM_PATCH;

        state <= REQ_WR_VER_MINOR;
      end
      REQ_WR_VER_MINOR: begin
        din <= {8'd0, params::VersionNumMinor};
        addr <= params::ADDR_VERSION_NUM_MINOR;

        state <= REQ_WR_VER_MAJOR;
      end
      REQ_WR_VER_MAJOR: begin
        din   <= {8'd0, params::VersionNumMajor};
        addr  <= params::ADDR_VERSION_NUM_MAJOR;

        state <= REQ_WR_FUNCTION_BITS;
      end
      REQ_WR_FUNCTION_BITS: begin
        din   <= {8'd0, FunctionBits};
        addr  <= params::ADDR_FUNCTION_BITS;

        state <= REQ_RD_CTL_FLAG;
      end
      WR_FPGA_STATE_RD_CTL_FLAG: begin
        addr <= params::ADDR_FPGA_STATE;
        if (fpga_state_din() != fpga_state_prev) begin
          we <= 1'b1;
          din <= fpga_state_din();
          fpga_state_prev <= fpga_state_din();
        end else begin
          we <= 1'b0;
        end

       ",
    );
    for (group, states) in &all_states {
        let name = group.name;
        write!(
            &mut source,
            r" if (ctl_flags[params::CTL_FLAG_BIT_{name}_SET]) begin
          ctl_flags[params::CTL_FLAG_BIT_{name}_SET] <= 1'b0;
          state <= {};
        end else",
            states[0].name
        )
        .unwrap();
    }
    source.push_str(
        r" begin
          ctl_flags_cand <= dout;
          if (dout == ctl_flags_cand) begin
            ctl_flags <= dout;
            set_result[params::SetResultTransitionInvalidBit] <= 1'b0;
          end
          state <= REQ_RD_CTL_FLAG;
        end
      end
      REQ_RD_CTL_FLAG: begin
        we <= 1'b0;
        addr <= params::ADDR_CTL_FLAG;
        state <= WR_FPGA_STATE_RD_CTL_FLAG;
      end
      SET_DONE_0: begin
        we <= 1'b0;
        state <= SET_DONE_1;
      end
      SET_DONE_1: begin
        addr <= params::ADDR_FPGA_STATE;
        state <= REQ_RD_CTL_FLAG;
      end
",
    );
    for (group, states) in &all_states {
        let name = group.name;
        let target = group.target();
        let len = states.len();
        for (i, state) in states.iter().enumerate() {
            write!(&mut source, "\n      {}: begin", state.name).unwrap();

            if i == 0 {
                source.push_str("\n        we <= 1'b0;");
            }

            if let Some(req) = &state.req {
                write!(&mut source, "\n        addr <= params::ADDR_{};", req.addr).unwrap();
            }

            if let Some(param) = &state.param {
                let width = param.width;
                let dout_range = if width == BRAM_DATA_WIDTH {
                    String::new()
                } else if width == 1 {
                    "[0]".to_string()
                } else {
                    format!("[{}:0]", width - 1)
                };
                write!(
                    &mut source,
                    "\n        {target}.{} <= dout{dout_range};",
                    param.name
                )
                .unwrap();
            }

            if i == len - 3
                && let Some(reject) = group.reject
            {
                write!(
                    &mut source,
                    "\n        set_result[params::CTL_FLAG_BIT_{name}_SET] <= {reject};"
                )
                .unwrap();
                if let Some(invalid) = group.transition_invalid {
                    write!(
                        &mut source,
                        "\n        set_result[params::SetResultTransitionInvalidBit] <= set_result[params::SetResultTransitionInvalidBit] | {invalid};"
                    )
                    .unwrap();
                }
            }

            if i == len - 2 {
                if group.reject.is_some() {
                    write!(
                        &mut source,
                        "\n        if (!set_result[params::CTL_FLAG_BIT_{name}_SET]) begin"
                    )
                    .unwrap();
                    for param in &group.params {
                        write!(
                            &mut source,
                            "\n          {name}_SETTINGS.{0} <= {target}.{0};",
                            param.name
                        )
                        .unwrap();
                    }
                    write!(
                        &mut source,
                        r"
          {name}_SETTINGS.UPDATE <= 1'b1;
        end
        we <= 1'b1;
        addr <= params::ADDR_SILENCER_SET_RESULT;
        din <= set_result;"
                    )
                    .unwrap();
                } else {
                    write!(&mut source, "\n        {name}_SETTINGS.UPDATE <= 1'b1;").unwrap();
                }
            }

            if i + 1 < len {
                write!(&mut source, "\n        state <= {};", states[i + 1].name).unwrap();
            }

            if i == len - 1 {
                write!(
                    &mut source,
                    r"
        {name}_SETTINGS.UPDATE <= 1'b0;
        we <= 1'b1;
        addr <= params::ADDR_CTL_FLAG;
        din <= ctl_flags;
        ctl_flags_cand <= ctl_flags;
        state <= SET_DONE_0;"
                )
                .unwrap();
            }

            source.push_str("\n      end");
        }
        source.push('\n');
    }
    source.push_str("\n      default: state <= WR_FPGA_STATE_RD_CTL_FLAG;\n    endcase\n  end\n");
    source.push_str("\n  initial begin");
    for group in &groups {
        write!(&mut source, "\n    {}_SETTINGS.UPDATE = 1'b0;", group.name).unwrap();
        for param in &group.params {
            write!(
                &mut source,
                "\n    {}_SETTINGS.{} = {};",
                group.name, param.name, param.default
            )
            .unwrap();
        }
    }
    source.push_str("\n  end\n");
    source.push_str("\nendmodule\n`default_nettype wire\n");
    let writer = std::thread::spawn(move || stdin.write_all(source.as_bytes()));
    let output = child
        .wait_with_output()
        .context("failed to wait for verible-verilog-format")?;
    writer
        .join()
        .map_err(|_| anyhow::anyhow!("the verible-verilog-format stdin writer panicked"))?
        .context("failed to feed verible-verilog-format")?;
    if !output.status.success() {
        bail!("verible-verilog-format exited with {}", output.status);
    }
    let contents = String::from_utf8(output.stdout)
        .context("verible-verilog-format wrote non-UTF-8 output")?;

    if check {
        let committed = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if committed != contents {
            let line = committed
                .lines()
                .zip(contents.lines())
                .position(|(a, b)| a != b)
                .map_or_else(
                    || committed.lines().count().min(contents.lines().count()) + 1,
                    |i| i + 1,
                );
            bail!(
                "{} differs from the generator output (first difference at line {line}); \
                 edit xtask/src/fpga_codegen.rs and run `cargo xtask fpga gen-controller`",
                path.display()
            );
        }
        println!("{} is up to date", path.display());
        return Ok(());
    }

    std::fs::write(&path, contents)
        .with_context(|| format!("failed to write {}", path.display()))?;
    println!("generated {}", path.display());
    Ok(())
}
