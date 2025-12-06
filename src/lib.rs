use common_game::components::planet::*;
use common_game::components::resource::{
    BasicResource, BasicResourceType, Combinator, ComplexResource, ComplexResourceRequest,
    ComplexResourceType, Generator, GenericResource,
};
use common_game::components::rocket::Rocket;
use common_game::protocols::messages::{
    ExplorerToPlanet, OrchestratorToPlanet, PlanetToExplorer, PlanetToOrchestrator,
};
use std::sync::mpsc;

struct PlanetCoreThinkingModel {
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
                // 1. Try to find a non-charged cell -> charge it with the Sunray
                if let Some(cell) = state.cells_iter_mut().find(|c| !c.is_charged()) {
                    cell.charge(sunray);
                } else {
                    // 2. All cells are full -> attempt to build the rocket only if allowed
                    if self.smart_rocket >= 1 && state.can_have_rocket() && !state.has_rocket() {
                        // Try building the rocket; if it fails, return None
                        if let Some(cell_index) = try_build_rocket(state) {
                            state.cell_mut(cell_index).charge(sunray);
                        } else {
                            return None;
                        }
                    }
                }

                // 3. Smart rocket = 2 -> always try to build the rocket if missing
                if self.smart_rocket == 2 && state.can_have_rocket() && !state.has_rocket() {
                    let _ = try_build_rocket(state); // ignore result intentionally
                }

                // 4. Send acknowledgement to the orchestrator
                Some(PlanetToOrchestrator::SunrayAck {
                    planet_id: state.id(),
                })
            }

            OrchestratorToPlanet::InternalStateRequest { .. } => {
                Some(PlanetToOrchestrator::InternalStateResponse {
                    planet_id: state.id(),
                    planet_state: PlanetState::to_dummy(state),
                })
            }
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
                    let Some((cell, _)) = state.full_cell() else {
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
            ExplorerToPlanet::CombineResourceRequest {
                explorer_id: _explorer_id,
                msg,
            } => {
                let Some((cell, _)) = state.full_cell() else {
                    return None;
                };

                match msg {
                    ComplexResourceRequest::Water(h, o) => {
                        let new_complex_resource = combinator
                            .make_water(h, o, cell)
                            .map(ComplexResource::Water)
                            .map_err(|(msg, h, o)| {
                                (
                                    msg,
                                    GenericResource::BasicResources(BasicResource::Hydrogen(h)),
                                    GenericResource::BasicResources(BasicResource::Oxygen(o)),
                                )
                            });

                        Some(PlanetToExplorer::CombineResourceResponse {
                            complex_response: new_complex_resource,
                        })
                    }
                    ComplexResourceRequest::Diamond(c1, c2) => {
                        let new_complex_resource = combinator
                            .make_diamond(c1, c2, cell)
                            .map(ComplexResource::Diamond)
                            .map_err(|(msg, c1, c2)| {
                                (
                                    msg,
                                    GenericResource::BasicResources(BasicResource::Carbon(c1)),
                                    GenericResource::BasicResources(BasicResource::Carbon(c2)),
                                )
                            });

                        Some(PlanetToExplorer::CombineResourceResponse {
                            complex_response: new_complex_resource,
                        })
                    }
                    ComplexResourceRequest::Life(w, c) => {
                        let new_complex_resource = combinator
                            .make_life(w, c, cell)
                            .map(ComplexResource::Life)
                            .map_err(|(msg, w, c)| {
                                (
                                    msg,
                                    GenericResource::ComplexResources(ComplexResource::Water(w)),
                                    GenericResource::BasicResources(BasicResource::Carbon(c)),
                                )
                            });

                        Some(PlanetToExplorer::CombineResourceResponse {
                            complex_response: new_complex_resource,
                        })
                    }
                    ComplexResourceRequest::Robot(s, l) => {
                        let new_complex_resource = combinator
                            .make_robot(s, l, cell)
                            .map(ComplexResource::Robot)
                            .map_err(|(msg, s, l)| {
                                (
                                    msg,
                                    GenericResource::BasicResources(BasicResource::Silicon(s)),
                                    GenericResource::ComplexResources(ComplexResource::Life(l)),
                                )
                            });

                        Some(PlanetToExplorer::CombineResourceResponse {
                            complex_response: new_complex_resource,
                        })
                    }
                    ComplexResourceRequest::Dolphin(w, l) => {
                        let new_complex_resource = combinator
                            .make_dolphin(w, l, cell)
                            .map(ComplexResource::Dolphin)
                            .map_err(|(msg, w, l)| {
                                (
                                    msg,
                                    GenericResource::ComplexResources(ComplexResource::Water(w)),
                                    GenericResource::ComplexResources(ComplexResource::Life(l)),
                                )
                            });

                        Some(PlanetToExplorer::CombineResourceResponse {
                            complex_response: new_complex_resource,
                        })
                    }
                    ComplexResourceRequest::AIPartner(r, d) => {
                        let new_complex_resource = combinator
                            .make_aipartner(r, d, cell)
                            .map(ComplexResource::AIPartner)
                            .map_err(|(msg, r, d)| {
                                (
                                    msg,
                                    GenericResource::ComplexResources(ComplexResource::Robot(r)),
                                    GenericResource::ComplexResources(ComplexResource::Diamond(d)),
                                )
                            });

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
        rocket
    }

    fn start(&mut self, state: &PlanetState) {
        self.running = true;
    }

    fn stop(&mut self, state: &PlanetState) {
        self.running = false;
    }
}

/// Tries to build a rocket using the first fully charged energy cell.
/// Returns `Some(index)` on success, or `None` on failure.
///
/// This helper extracts a full cell through `state.full_cell()`, which provides
/// both the mutable reference and its index. If no full cell exists or the
/// rocket cannot be built, the function returns `None`.
fn try_build_rocket(state: &mut PlanetState) -> Option<usize> {
    let Some((_, cell_index)) = state.full_cell() else {
        return None;
    };
    state.build_rocket(cell_index).ok()?; // if Err -> return None

    Some(cell_index)
}

/// Creates and initializes a new `Planet` instance with a predefined set of
/// generation and combination rules, a basic AI model, and the communication
/// channels used to interact with the orchestrator and explorers.
///
/// Planet configuration
/// - Type: C
/// - Generation rule: Oxygen
/// - Combination rules: Diamond, Water, Life, Robot, Dolphin, AIPartner
///
/// Parameters
/// - The channels used to receive messages from the orchestrator and
///   send responses back
/// - The channel used to receive messages from explorers
/// - planet_id: the id of the planet
/// - smart_rocket:
///     - 0: default behaviour
///     - 1: use exceeding sunray to generate a rocket when missing one
///     - 2: always generate rocket when missing one
///
///
/// Returns:
/// - `Ok(Planet)` if the configuration is valid for the selected planet type
/// - `Err(String)` if the rules exceed the constraints of the planet type
///
/// Note:
/// The returned planet is created in a *stopped* state. To start it, spawn
/// a thread and call `planet.run()`, then send a
/// `OrchestratorToPlanet::StartPlanetAI` message.

pub fn new_planet(
    rx_orchestrator: mpsc::Receiver<OrchestratorToPlanet>,
    tx_orchestrator: mpsc::Sender<PlanetToOrchestrator>,
    rx_explorer: mpsc::Receiver<ExplorerToPlanet>,
    planet_id: u32,
    smart_rocket: u8,
) -> Result<Planet, String> {
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
        planet_id,
        PlanetType::C,
        Box::new(ai),
        gen_rules,
        comb_rules,
        (rx_orchestrator, tx_orchestrator),
        rx_explorer,
    )
}
