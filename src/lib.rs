use common_game::components::planet::*;
use common_game::components::resource::{
    BasicResource, BasicResourceType, Combinator, ComplexResource, ComplexResourceRequest,
    ComplexResourceType, Generator,
};
use common_game::components::rocket::Rocket;
use common_game::protocols::messages::{
    ExplorerToPlanet, OrchestratorToPlanet, PlanetToExplorer, PlanetToOrchestrator,
};
use std::sync::mpsc;

struct PlanetCoreThinkingModel {
    //TODO rename
    smart_rocket: u8,
    running: bool,
}
impl PlanetAI for PlanetCoreThinkingModel {
    fn handle_orchestrator_msg(
        &mut self,
        state: &mut PlanetState,
        generator: &Generator,
        combinator: &Combinator,
        msg: OrchestratorToPlanet,
    ) -> Option<PlanetToOrchestrator> {
        match msg {
            OrchestratorToPlanet::Sunray(sunray) => {
                match state.cells_iter_mut().find(|c| !c.is_charged()) {
                    Some(cell) => {
                        // Caso: c’è una cella libera -> la carico
                        cell.charge(sunray);
                    }
                    None => {
                        // Caso: tutte le celle sono cariche -> prova a costruire il razzo
                        if self.smart_rocket == 1 && state.can_have_rocket() && !state.has_rocket()
                        {
                            let cell_number = state.cells_count() - 1;
                            state.build_rocket(cell_number);
                            state.cell_mut(cell_number).charge(sunray);
                        }
                    }
                }
                Some(PlanetToOrchestrator::SunrayAck {
                    planet_id: state.id(),
                })
            }
            // OrchestratorToPlanet::InternalStateRequest() => {
            //     Some(PlanetToOrchestrator::InternalStateResponse {
            //         planet_id: state.id(),
            //         planet_state: state //TODO
            //     })
            // }
            //OrchestratorToPlanet::Asteroid(_) => {}//handle_asteroid
            // OrchestratorToPlanet::StartPlanetAI(_) => {}//start
            // OrchestratorToPlanet::StopPlanetAI(_) => {}//stop
            _ => None,
        }
    }

    fn handle_explorer_msg(
        &mut self,
        state: &mut PlanetState,
        generator: &Generator,
        combinator: &Combinator,
        msg: ExplorerToPlanet,
    ) -> Option<PlanetToExplorer> {
        match msg {
            ExplorerToPlanet::SupportedResourceRequest { .. } => {
                Some(PlanetToExplorer::SupportedResourceResponse {
                    resource_list: generator.all_available_recipes(),
                })
            }
            ExplorerToPlanet::SupportedCombinationRequest { .. } => {
                Some(PlanetToExplorer::SupportedCombinationResponse {
                    combination_list: combinator.all_available_recipes(),
                })
            }
            ExplorerToPlanet::GenerateResourceRequest {
                explorer_id,
                resource,
            } => match resource {
                BasicResourceType::Oxygen => {
                    let Some((cell, indx)) = state.full_cell() else {
                        return None;
                    };

                    let new_basic_resource =
                        generator.make_oxygen(cell).ok().map(BasicResource::Oxygen);

                    Some(PlanetToExplorer::GenerateResourceResponse {
                        resource: new_basic_resource,
                    })
                }
                _ => None,
            },
            ExplorerToPlanet::CombineResourceRequest { explorer_id, msg } => {
                let Some((cell, indx)) = state.full_cell() else {
                    return None;
                };

                match msg {
                    ComplexResourceRequest::Water(h, o) => {
                        let new_complex_resource = combinator
                            .make_water(h, o, cell)
                            .ok()
                            .map(ComplexResource::Water);

                        Some(PlanetToExplorer::CombineResourceResponse {
                            complex_response: new_complex_resource,
                        })
                    }
                    ComplexResourceRequest::Diamond(c1, c2) => {
                        let new_complex_resource = combinator
                            .make_diamond(c1, c2, cell)
                            .ok()
                            .map(ComplexResource::Diamond);

                        Some(PlanetToExplorer::CombineResourceResponse {
                            complex_response: new_complex_resource,
                        })
                    }
                    ComplexResourceRequest::Life(w, c) => {
                        let new_complex_resource = combinator
                            .make_life(w, c, cell)
                            .ok()
                            .map(ComplexResource::Life);

                        Some(PlanetToExplorer::CombineResourceResponse {
                            complex_response: new_complex_resource,
                        })
                    }
                    ComplexResourceRequest::Robot(s, l) => {
                        let new_complex_resource = combinator
                            .make_robot(s, l, cell)
                            .ok()
                            .map(ComplexResource::Robot);

                        Some(PlanetToExplorer::CombineResourceResponse {
                            complex_response: new_complex_resource,
                        })
                    }
                    ComplexResourceRequest::Dolphin(w, l) => {
                        let new_complex_resource = combinator
                            .make_dolphin(w, l, cell)
                            .ok()
                            .map(ComplexResource::Dolphin);

                        Some(PlanetToExplorer::CombineResourceResponse {
                            complex_response: new_complex_resource,
                        })
                    }
                    ComplexResourceRequest::AIPartner(r, d) => {
                        let new_complex_resource = combinator
                            .make_aipartner(r, d, cell)
                            .ok()
                            .map(ComplexResource::AIPartner);

                        Some(PlanetToExplorer::CombineResourceResponse {
                            complex_response: new_complex_resource,
                        })
                    }
                }
            }
            ExplorerToPlanet::AvailableEnergyCellRequest { .. } => {
                Some(PlanetToExplorer::AvailableEnergyCellResponse {
                    available_cells: state.cells_count() as u32,
                })
            }
        }
    }

    fn handle_asteroid(
        &mut self,
        state: &mut PlanetState,
        generator: &Generator,
        combinator: &Combinator,
    ) -> Option<Rocket> {
        if !state.can_have_rocket() {
            return None;
        }
        if !state.has_rocket() {
            return None;
        }
        let rocket = state.take_rocket();
        if self.smart_rocket == 2 {
            let cell_number = state.cells_count() - 1;
            let res = state.build_rocket(cell_number);
        }
        rocket
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

pub fn new_planet(
    rx_orchestrator: mpsc::Receiver<OrchestratorToPlanet>,
    tx_orchestrator: mpsc::Sender<PlanetToOrchestrator>,
    rx_explorer: mpsc::Receiver<ExplorerToPlanet>,
    tx_explorer: mpsc::Sender<PlanetToExplorer>,
    smart_rocket: u8,
) -> Result<Planet, String> {
    let id = 1;
    let ai = PlanetCoreThinkingModel {
        smart_rocket,
        running: false,
    };
    let gen_rules = vec![
        BasicResourceType::Oxygen,
        // BasicResourceType::Hydrogen,
        // BasicResourceType::Carbon,
        // BasicResourceType::Silicon,
    ];

    let comb_rules = vec![
        ComplexResourceType::Diamond,
        ComplexResourceType::Water,
        ComplexResourceType::Life,
        ComplexResourceType::Robot,
        ComplexResourceType::Dolphin,
        ComplexResourceType::AIPartner,
    ];

    Planet::new(
        id,
        PlanetType::C,
        Box::new(ai),
        gen_rules,
        comb_rules,
        (rx_orchestrator, tx_orchestrator),
        (rx_explorer, tx_explorer),
    )
}
