use common_game::components::planet::*;
use common_game::components::resource::{Combinator, Generator};
use common_game::components::rocket::Rocket;
use common_game::protocols::messages::{ExplorerToPlanet, OrchestratorToPlanet, PlanetToExplorer, PlanetToOrchestrator};
use std::sync::mpsc;
use std::time::SystemTime;

struct PlanetCoreThinkingModel{ //TODO rename
    smart_rocket: u8,
    running: bool,
}
impl PlanetAI for PlanetCoreThinkingModel{
    fn handle_orchestrator_msg(&mut self, state: &mut PlanetState, generator: &Generator, combinator: &Combinator, msg: OrchestratorToPlanet) -> Option<PlanetToOrchestrator> {
        match msg {
            OrchestratorToPlanet::Sunray(sunray) => {
                match state.cells_iter_mut().find(|c| !c.is_charged()) {
                    Some(cell) => {
                        // Caso: c’è una cella libera -> la carico
                        cell.charge(sunray);
                    }
                    None => {
                        // Caso: tutte le celle sono cariche -> prova a costruire il razzo
                        if self.smart_rocket == 1 && state.can_have_rocket() && !state.has_rocket() {
                            let cell_number = state.cells_count();
                            state.build_rocket(cell_number);
                            state.cell_mut(cell_number).charge(sunray);
                        }
                    }
                }
                Some(PlanetToOrchestrator::SunrayAck {
                    planet_id: state.id(),
                    timestamp: SystemTime::now(),
                })

            }
            OrchestratorToPlanet::Asteroid(_) => {}//handle_asteroid
            OrchestratorToPlanet::StartPlanetAI(_) => {}//start
            OrchestratorToPlanet::StopPlanetAI(_) => {}//stop
            OrchestratorToPlanet::InternalStateRequest(_) => {}
        }

    }

    fn handle_explorer_msg(&mut self, state: &mut PlanetState, generator: &Generator, combinator: &Combinator, msg: ExplorerToPlanet) -> Option<PlanetToExplorer> {
        match msg {
            ExplorerToPlanet::SupportedResourceRequest { .. } => {}
            ExplorerToPlanet::SupportedCombinationRequest { .. } => {}
            ExplorerToPlanet::GenerateResourceRequest { .. } => {}
            ExplorerToPlanet::CombineResourceRequest { .. } => {}
            ExplorerToPlanet::AvailableEnergyCellRequest { .. } => {}
            ExplorerToPlanet::InternalStateRequest { .. } => {}
        }
        todo!()
    }

    fn handle_asteroid(&mut self, state: &mut PlanetState, generator: &Generator, combinator: &Combinator) -> Option<Rocket> {
        if !state.can_have_rocket() {
            return None;
        }
        if !state.has_rocket() {
            return None;
        }
        let roket = state.take_rocket();
        if self.smart_rocket == 2 {
            let cell_number = state.cells_count();
            let res = state.build_rocket(cell_number);
        }
        roket

    }

    fn start(&mut self, state: &PlanetState) {
        self.running = true;
        todo!()
    }

    fn stop(&mut self, state: &PlanetState) {
        self.running = false;
        todo!()
    }
}

/*fn new_planet(smart_rocket: u8) -> Result<Planet<PlanetCoreThinkingModel>, String> {

    Planet::new(/* u32 */, /* PlanetType */, /* ai */, /* Vec<BasicResourceType> */, /* Vec<ComplexResourceType> */, /* (std::sync::mpsc::Receiver<OrchestratorToPlanet>, std::sync::mpsc::Sender<PlanetToOrchestrator>) */, /* (std::sync::mpsc::Receiver<ExplorerToPlanet>, std::sync::mpsc::Sender<PlanetToExplorer>) */)
}*/
pub fn new_planet(
    rx_orchestrator: mpsc::Receiver<OrchestratorToPlanet>,
    tx_orchestrator: mpsc::Sender<PlanetToOrchestrator>,
    rx_explorer: mpsc::Receiver<ExplorerToPlanet>,
    tx_explorer: mpsc::Sender<PlanetToExplorer>,
    smart_rocket: u8
) -> Result<Planet<PlanetCoreThinkingModel>, String> {

    let id = 1;
    let ai = PlanetCoreThinkingModel {
        smart_rocket, running: false,
    };
    let gen_rules = vec![/* your recipes */];
    let comb_rules = vec![/* your recipes */];


    Planet::new(
        id,
        PlanetType::A,
        ai,
        gen_rules,
        comb_rules,
        (rx_orchestrator, tx_orchestrator),
        (rx_explorer, tx_explorer)
    )
}