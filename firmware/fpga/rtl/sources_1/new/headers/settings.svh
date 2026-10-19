package settings;

  typedef struct {
    logic UPDATE;
    logic REQ_RD_BANK;
    logic [7:0] TRANSITION_MODE;
    logic [63:0] TRANSITION_VALUE;
    logic [15:0] CYCLE[2];
    logic [15:0] FREQ_DIV[2];
    logic [15:0] REP[2];
  } mod_settings_t;

  typedef struct {
    logic UPDATE;
    logic REQ_RD_BANK;
    logic [7:0] TRANSITION_MODE;
    logic [63:0] TRANSITION_VALUE;
    logic MODE[2];
    logic [15:0] CYCLE[2];
    logic [15:0] FREQ_DIV[2];
    logic [15:0] REP[2];
    logic [15:0] SOUND_SPEED[2];
    logic [7:0] NUM_FOCI[2];
  } pattern_settings_t;

  typedef struct {
    logic        UPDATE;
    logic [7:0]  FLAG;
    logic [15:0] UPDATE_RATE_INTENSITY;
    logic [15:0] UPDATE_RATE_PHASE;
    logic [15:0] COMPLETION_STEPS_INTENSITY;
    logic [15:0] COMPLETION_STEPS_PHASE;
  } silencer_settings_t;

  typedef struct {
    logic UPDATE;
    logic [63:0] SYNC_TIME;
  } sync_settings_t;

  typedef struct {
    logic UPDATE;
    logic [63:0] VALUE[4];
  } debug_settings_t;

  typedef struct {
    logic UPDATE;
    logic ENABLE;
    logic MANUAL;
    logic [15:0] KP;
    logic [15:0] KI;
    logic [15:0] KF;
    logic signed [15:0] MANUAL_RATE;
  } sync_ps_settings_t;

  typedef struct {
    logic FINE;
    logic [7:0] FINE_EXIT_COUNT;
    logic signed [15:0] RATE;
    logic signed [15:0] INTEG;
    logic [15:0] PD_POS;
    logic [15:0] PD_NEG;
    logic signed [15:0] EDGE_DIFF;
    logic [15:0] EDGE_COUNT;
  } sync_ps_state_t;

endpackage
