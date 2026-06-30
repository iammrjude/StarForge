//! Integration tests for the local network simulation environment.

use starforge::utils::network_sim::{
    builtin_scenarios, FailureMode, NetworkSimulator, SimScenario, SimScenarioStep,
};

#[test]
fn simulator_deterministic_across_runs() {
    let mut sim_a = NetworkSimulator::new(12345);
    let mut sim_b = NetworkSimulator::new(12345);

    let id_a = sim_a.deploy_contract("wasm_hash_abc").unwrap();
    let id_b = sim_b.deploy_contract("wasm_hash_abc").unwrap();
    assert_eq!(id_a, id_b);

    let res_a = sim_a.invoke(&id_a, "increment", &[]).unwrap();
    let res_b = sim_b.invoke(&id_b, "increment", &[]).unwrap();
    assert_eq!(res_a.return_value, res_b.return_value);
    assert_eq!(res_a.fee, res_b.fee);
}

#[test]
fn snapshot_and_restore_roundtrip() {
    let mut sim = NetworkSimulator::new(1);
    sim.deploy_contract_with_id("C_TEST", "hash").unwrap();
    sim.fund_account("GACC", 5000);
    sim.snapshot("checkpoint");

    sim.fund_account("GACC", 10000);
    assert_eq!(sim.state().accounts.get("GACC"), Some(&15000));

    sim.restore("checkpoint").unwrap();
    assert_eq!(sim.state().accounts.get("GACC"), Some(&5000));
}

#[test]
fn failure_injection_blocks_deploy() {
    let mut sim = NetworkSimulator::new(42);
    sim.set_failure_mode(FailureMode::RpcError);
    assert!(sim.deploy_contract("hash").is_err());
}

#[test]
fn time_control_advances_state() {
    let mut sim = NetworkSimulator::new(1);
    let ts = sim.state().timestamp;
    let ledger = sim.state().ledger_sequence;
    sim.advance_time(120);
    sim.advance_ledger(10);
    assert_eq!(sim.state().timestamp, ts + 120);
    assert_eq!(sim.state().ledger_sequence, ledger + 10);
}

#[test]
fn builtin_scenarios_all_pass() {
    for scenario in builtin_scenarios() {
        let mut sim = NetworkSimulator::new(scenario.seed);
        let result = sim.run_scenario(&scenario);
        assert!(
            result.passed,
            "Scenario '{}' failed: {:?}",
            scenario.name,
            result.errors
        );
    }
}

#[test]
fn custom_scenario_with_expected_return() {
    let scenario = SimScenario {
        name: "custom".to_string(),
        description: "test".to_string(),
        seed: 1,
        initial_ledger: 1,
        steps: vec![
            SimScenarioStep::Deploy {
                contract_id: "C_CUSTOM".to_string(),
                wasm_hash: "hash".to_string(),
            },
            SimScenarioStep::Invoke {
                contract_id: "C_CUSTOM".to_string(),
                function: "get".to_string(),
                args: vec![],
                expected_return: None,
            },
        ],
    };
    let mut sim = NetworkSimulator::new(1);
    let result = sim.run_scenario(&scenario);
    assert!(result.passed);
}
