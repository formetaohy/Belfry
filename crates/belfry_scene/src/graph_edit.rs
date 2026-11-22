use crate::graph::{GraphAsset, GraphEntry, GraphState};
use crate::reference::AnimationId;
use crate::scene::Scene;
use crate::target::{Target, valid_id};
use belfry_animation::{Comparison, Parameter};

impl Scene {
    pub fn add_graph(&mut self, id: &str) -> Result<Target, String> {
        if !valid_id(id) {
            return Err(format!("the graph id {id} holds no letter, digit, _ or -"));
        }
        if self.graph_index(id).is_some() {
            return Err(format!("the scene holds the graph {id}"));
        }
        let clip = self
            .animations
            .first()
            .ok_or_else(|| String::from("the scene holds no animation to graph"))?;
        let state = GraphState::new("default", AnimationId::new(clip.id.as_str()), 1.0, true)?;
        self.graphs.push(GraphEntry::new(
            id,
            GraphAsset::new("default", Vec::new(), vec![state], Vec::new()),
        ));
        Ok(Target::Graph(String::from(id)))
    }

    pub fn add_graph_state(&mut self, graph: &str, id: &str, clip: &str) -> Result<(), String> {
        if self.animation_index(clip).is_none() {
            return Err(format!("the scene holds no animation {clip}"));
        }
        self.graph_mut(graph)?.asset.add_state(id, clip, 1.0, true)
    }

    pub fn remove_graph_state(&mut self, graph: &str, id: &str) -> Result<(), String> {
        self.graph_mut(graph)?.asset.remove_state(id)
    }

    pub fn add_graph_transition(
        &mut self,
        graph: &str,
        from: &str,
        to: &str,
        fade: f32,
        exit: f32,
    ) -> Result<(), String> {
        self.graph_mut(graph)?
            .asset
            .add_transition(from, to, fade, exit)
    }

    pub fn remove_graph_transition(&mut self, graph: &str, index: usize) -> Result<(), String> {
        self.graph_mut(graph)?.asset.remove_transition(index)
    }

    pub fn add_graph_parameter(
        &mut self,
        graph: &str,
        name: &str,
        default: Parameter,
    ) -> Result<(), String> {
        self.graph_mut(graph)?.asset.add_parameter(name, default)
    }

    pub fn remove_graph_parameter(&mut self, graph: &str, name: &str) -> Result<(), String> {
        self.graph_mut(graph)?.asset.remove_parameter(name)
    }

    pub fn add_graph_condition(
        &mut self,
        graph: &str,
        transition: usize,
        parameter: &str,
        comparison: Comparison,
        value: Parameter,
    ) -> Result<(), String> {
        self.graph_mut(graph)?
            .asset
            .add_condition(transition, parameter, comparison, value)
    }

    pub fn remove_graph_condition(
        &mut self,
        graph: &str,
        transition: usize,
        index: usize,
    ) -> Result<(), String> {
        self.graph_mut(graph)?
            .asset
            .remove_condition(transition, index)
    }

    fn graph_mut(&mut self, id: &str) -> Result<&mut GraphEntry, String> {
        if let Some(prefab) = self.asset_owner(crate::schema::ReferenceKind::Graph, id) {
            return Err(format!(
                "the graph {id} belongs to the prefab {}",
                prefab.display(),
            ));
        }
        self.graphs
            .iter_mut()
            .find(|held| held.id == id)
            .ok_or_else(|| format!("the scene holds no graph {id}"))
    }
}
