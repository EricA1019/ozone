use std::collections::BTreeMap;

use ozone_core::engine::ConversationMessage;

#[derive(Debug, Clone, PartialEq)]
pub struct ContextLayerPolicy {
    pub layers: Vec<ContextLayer>,
}

impl ContextLayerPolicy {
    pub fn phase1e_defaults() -> Self {
        Self {
            layers: vec![
                ContextLayer {
                    kind: ContextLayerKind::SystemPrompt,
                    priority: 0,
                    min_budget_pct: 5.0,
                    max_budget_pct: 20.0,
                    is_hard_context: true,
                    collapse_strategy: CollapseStrategy::Never,
                },
                ContextLayer {
                    kind: ContextLayerKind::CharacterCard,
                    priority: 1,
                    min_budget_pct: 10.0,
                    max_budget_pct: 35.0,
                    is_hard_context: true,
                    collapse_strategy: CollapseStrategy::Never,
                },
                ContextLayer {
                    kind: ContextLayerKind::PinnedMemory,
                    priority: 2,
                    min_budget_pct: 5.0,
                    max_budget_pct: 20.0,
                    is_hard_context: false,
                    collapse_strategy: CollapseStrategy::TruncateTail,
                },
                ContextLayer {
                    kind: ContextLayerKind::RecentMessages,
                    priority: 3,
                    min_budget_pct: 20.0,
                    max_budget_pct: 55.0,
                    is_hard_context: false,
                    collapse_strategy: CollapseStrategy::OmitOldest,
                },
                ContextLayer {
                    kind: ContextLayerKind::RetrievedMemory,
                    priority: 4,
                    min_budget_pct: 10.0,
                    max_budget_pct: 30.0,
                    is_hard_context: false,
                    collapse_strategy: CollapseStrategy::TruncateTail,
                },
                ContextLayer {
                    kind: ContextLayerKind::LorebookEntries,
                    priority: 5,
                    min_budget_pct: 5.0,
                    max_budget_pct: 20.0,
                    is_hard_context: false,
                    collapse_strategy: CollapseStrategy::TruncateTail,
                },
                ContextLayer {
                    kind: ContextLayerKind::ThinkingSummary,
                    priority: 6,
                    min_budget_pct: 0.0,
                    max_budget_pct: 20.0,
                    is_hard_context: false,
                    collapse_strategy: CollapseStrategy::Summarize,
                },
                ContextLayer {
                    kind: ContextLayerKind::SessionSynopsis,
                    priority: 7,
                    min_budget_pct: 0.0,
                    max_budget_pct: 15.0,
                    is_hard_context: false,
                    collapse_strategy: CollapseStrategy::TruncateHead,
                },
            ],
        }
    }

    fn active_layers<'a>(&'a self, inputs: &'a [ContextLayerInput]) -> Vec<&'a ContextLayer> {
        let mut input_kinds = BTreeMap::new();
        for input in inputs {
            input_kinds.insert(input.kind, ());
        }

        self.layers
            .iter()
            .filter(|layer| input_kinds.contains_key(&layer.kind))
            .collect()
    }

    fn layer_for(&self, kind: ContextLayerKind) -> Option<&ContextLayer> {
        self.layers.iter().find(|layer| layer.kind == kind)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContextLayer {
    pub kind: ContextLayerKind,
    pub priority: u8,
    pub min_budget_pct: f32,
    pub max_budget_pct: f32,
    pub is_hard_context: bool,
    pub collapse_strategy: CollapseStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContextLayerKind {
    SystemPrompt,
    CharacterCard,
    PinnedMemory,
    RecentMessages,
    RetrievedMemory,
    LorebookEntries,
    ThinkingSummary,
    SessionSynopsis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollapseStrategy {
    TruncateTail,
    TruncateHead,
    OmitOldest,
    Summarize,
    Never,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContextLayerInput {
    pub kind: ContextLayerKind,
    pub items: Vec<ContextItemInput>,
}

impl ContextLayerInput {
    pub fn from_transcript(kind: ContextLayerKind, transcript: &[ConversationMessage]) -> Self {
        let items = transcript
            .iter()
            .enumerate()
            .map(|(index, message)| ContextItemInput {
                id: message.message_id.to_string(),
                description: format!("{}:{}", message.author_kind, message.message_id),
                content: format!("{}: {}", message.author_kind, message.content),
                sequence: index as i64,
                priority_score: None,
                is_stale: false,
                user_excluded: false,
            })
            .collect();

        Self { kind, items }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContextItemInput {
    pub id: String,
    pub description: String,
    pub content: String,
    pub sequence: i64,
    pub priority_score: Option<f32>,
    pub is_stale: bool,
    pub user_excluded: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContextPlan {
    pub id: String,
    pub layers: Vec<ContextPlanLayer>,
    pub total_tokens: usize,
    pub budget: usize,
    pub safety_margin_tokens: usize,
    pub estimation_policy: TokenEstimationPolicy,
    pub estimation_confidence: TokenEstimationConfidence,
    pub is_dry_run: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContextPlanLayer {
    pub kind: ContextLayerKind,
    pub content: String,
    pub token_count: usize,
    pub allocated_budget_tokens: usize,
    pub is_hard_context: bool,
    pub was_truncated: bool,
    pub truncation_reason: Option<String>,
    pub items_included: usize,
    pub items_omitted: usize,
    pub omitted_items: Vec<OmittedItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OmittedItem {
    pub item_id: String,
    pub description: String,
    pub token_count: usize,
    pub reason: OmissionReason,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OmissionReason {
    BudgetExceeded,
    PriorityTooLow { score: f32 },
    StaleArtifact,
    UserExcluded,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenEstimationPolicy {
    Exact {
        tokenizer: String,
    },
    Approximate {
        model_family: String,
        calibration_ratio: f32,
    },
    Heuristic {
        chars_per_token: f32,
        language_hint: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenEstimationConfidence {
    Exact,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TokenSafetyMargins {
    pub exact_pct: f32,
    pub approximate_pct: f32,
    pub heuristic_pct: f32,
}

impl Default for TokenSafetyMargins {
    fn default() -> Self {
        Self {
            exact_pct: 0.0,
            approximate_pct: 0.08,
            heuristic_pct: 0.2,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExactEstimator {
    pub tokenizer: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApproximateEstimator {
    pub model_family: String,
    pub calibration_ratio: f32,
    pub calibrated: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeuristicEstimator {
    pub chars_per_token: f32,
    pub language_hint: Option<String>,
}

impl Default for HeuristicEstimator {
    fn default() -> Self {
        Self {
            chars_per_token: 3.0,
            language_hint: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TokenEstimatorChain {
    pub exact: Option<ExactEstimator>,
    pub approximate: Option<ApproximateEstimator>,
    pub heuristic: HeuristicEstimator,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContextPlanRequest {
    pub budget_tokens: usize,
    pub created_at: i64,
    pub is_dry_run: bool,
    pub plan_id: Option<String>,
    pub safety_margin_tokens: Option<usize>,
    pub layers: Vec<ContextLayerInput>,
}

impl ContextPlanRequest {
    pub fn new(budget_tokens: usize, created_at: i64, layers: Vec<ContextLayerInput>) -> Self {
        Self {
            budget_tokens,
            created_at,
            is_dry_run: false,
            plan_id: None,
            safety_margin_tokens: None,
            layers,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptPlanRequest {
    pub budget_tokens: usize,
    pub created_at: i64,
    pub is_dry_run: bool,
    pub plan_id: Option<String>,
    pub safety_margin_tokens: Option<usize>,
    pub prepend_layers: Vec<ContextLayerInput>,
}

impl TranscriptPlanRequest {
    pub fn new(budget_tokens: usize, created_at: i64) -> Self {
        Self {
            budget_tokens,
            created_at,
            is_dry_run: false,
            plan_id: None,
            safety_margin_tokens: None,
            prepend_layers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContextAssemblyError {
    InvalidPolicy {
        reason: String,
    },
    HardContextOverflow {
        layer: ContextLayerKind,
        needed: usize,
        available: usize,
    },
}

impl std::fmt::Display for ContextAssemblyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPolicy { reason } => write!(f, "invalid context policy: {reason}"),
            Self::HardContextOverflow {
                layer,
                needed,
                available,
            } => write!(
                f,
                "hard context overflow in {layer:?}: requires {needed} tokens, budget allows {available}"
            ),
        }
    }
}

impl std::error::Error for ContextAssemblyError {}

#[derive(Debug, Clone)]
pub struct ContextAssembler {
    policy: ContextLayerPolicy,
    estimator_chain: TokenEstimatorChain,
    safety_margins: TokenSafetyMargins,
}

impl Default for ContextAssembler {
    fn default() -> Self {
        Self::new(
            ContextLayerPolicy::phase1e_defaults(),
            TokenEstimatorChain::default(),
            TokenSafetyMargins::default(),
        )
    }
}

impl ContextAssembler {
    pub fn new(
        policy: ContextLayerPolicy,
        estimator_chain: TokenEstimatorChain,
        safety_margins: TokenSafetyMargins,
    ) -> Self {
        Self {
            policy,
            estimator_chain,
            safety_margins,
        }
    }

    pub fn policy(&self) -> &ContextLayerPolicy {
        &self.policy
    }

    pub fn build_plan(
        &self,
        request: ContextPlanRequest,
    ) -> Result<ContextPlan, ContextAssemblyError> {
        let active_estimator = self.estimator_chain.active_estimator();
        let policy_layers = self.policy.active_layers(&request.layers);
        self.validate_policy(&policy_layers)?;

        let safety_margin_tokens = request.safety_margin_tokens.unwrap_or_else(|| {
            pct_of_tokens(
                request.budget_tokens,
                active_estimator.safety_margin_pct(&self.safety_margins),
            )
        });
        let available_budget = request.budget_tokens.saturating_sub(safety_margin_tokens);

        let layer_allocations = allocate_layer_budgets(&policy_layers, available_budget)?;
        let mut estimated_inputs = estimate_layer_inputs(&request.layers, active_estimator);

        let mut hard_plans = BTreeMap::new();
        let mut hard_total = 0usize;

        for layer in &policy_layers {
            if !layer.is_hard_context {
                continue;
            }

            let estimated_items = estimated_inputs.remove(&layer.kind).unwrap_or_default();
            let plan_layer = build_hard_layer_plan(layer, &estimated_items);
            hard_total = hard_total.saturating_add(plan_layer.token_count);

            if hard_total > available_budget {
                return Err(ContextAssemblyError::HardContextOverflow {
                    layer: layer.kind,
                    needed: hard_total,
                    available: available_budget,
                });
            }

            hard_plans.insert(layer.kind, plan_layer);
        }

        let mut remaining_soft_budget = available_budget.saturating_sub(hard_total);
        let mut soft_plans = BTreeMap::new();

        let mut soft_layers = policy_layers
            .iter()
            .copied()
            .filter(|layer| !layer.is_hard_context)
            .collect::<Vec<_>>();
        soft_layers.sort_by_key(|left| left.priority);

        for layer in soft_layers {
            let estimated_items = estimated_inputs.remove(&layer.kind).unwrap_or_default();
            let allocated = layer_allocations
                .get(&layer.kind)
                .copied()
                .unwrap_or_default();
            let layer_budget = allocated.min(remaining_soft_budget);
            let plan_layer =
                build_soft_layer_plan(layer, &estimated_items, layer_budget, active_estimator);
            remaining_soft_budget = remaining_soft_budget.saturating_sub(plan_layer.token_count);
            soft_plans.insert(layer.kind, plan_layer);
        }

        let mut layers = Vec::new();
        for layer in policy_layers {
            if let Some(hard_plan) = hard_plans.remove(&layer.kind) {
                layers.push(hard_plan);
                continue;
            }
            if let Some(soft_plan) = soft_plans.remove(&layer.kind) {
                layers.push(soft_plan);
            }
        }

        let total_tokens = layers.iter().map(|layer| layer.token_count).sum::<usize>();
        let plan_id = request
            .plan_id
            .unwrap_or_else(|| format!("context-plan-{}", request.created_at));

        Ok(ContextPlan {
            id: plan_id,
            layers,
            total_tokens,
            budget: request.budget_tokens,
            safety_margin_tokens,
            estimation_policy: active_estimator.policy(),
            estimation_confidence: active_estimator.confidence(),
            is_dry_run: request.is_dry_run,
            created_at: request.created_at,
        })
    }

    pub fn build_plan_from_transcript(
        &self,
        transcript: &[ConversationMessage],
        request: TranscriptPlanRequest,
    ) -> Result<ContextPlan, ContextAssemblyError> {
        let mut layers = request.prepend_layers;
        layers.push(ContextLayerInput::from_transcript(
            ContextLayerKind::RecentMessages,
            transcript,
        ));

        self.build_plan(ContextPlanRequest {
            budget_tokens: request.budget_tokens,
            created_at: request.created_at,
            is_dry_run: request.is_dry_run,
            plan_id: request.plan_id,
            safety_margin_tokens: request.safety_margin_tokens,
            layers,
        })
    }

    fn validate_policy(&self, policy_layers: &[&ContextLayer]) -> Result<(), ContextAssemblyError> {
        let min_sum = policy_layers
            .iter()
            .map(|layer| layer.min_budget_pct)
            .sum::<f32>();
        if min_sum > 100.0 {
            return Err(ContextAssemblyError::InvalidPolicy {
                reason: format!(
                    "minimum budget percentages exceed 100% ({min_sum:.2}%) for active layers"
                ),
            });
        }

        for layer in policy_layers {
            if !(0.0..=100.0).contains(&layer.min_budget_pct)
                || !(0.0..=100.0).contains(&layer.max_budget_pct)
            {
                return Err(ContextAssemblyError::InvalidPolicy {
                    reason: format!(
                        "layer {:?} uses out-of-range percentages ({:.2}, {:.2})",
                        layer.kind, layer.min_budget_pct, layer.max_budget_pct
                    ),
                });
            }
            if layer.min_budget_pct > layer.max_budget_pct {
                return Err(ContextAssemblyError::InvalidPolicy {
                    reason: format!(
                        "layer {:?} has min_budget_pct > max_budget_pct ({:.2} > {:.2})",
                        layer.kind, layer.min_budget_pct, layer.max_budget_pct
                    ),
                });
            }
            if self.policy.layer_for(layer.kind).is_none() {
                return Err(ContextAssemblyError::InvalidPolicy {
                    reason: format!("layer {:?} is missing from policy", layer.kind),
                });
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct ActiveEstimator<'a> {
    chain: &'a TokenEstimatorChain,
}

impl TokenEstimatorChain {
    fn active_estimator(&self) -> ActiveEstimator<'_> {
        ActiveEstimator { chain: self }
    }
}

impl ActiveEstimator<'_> {
    fn policy(&self) -> TokenEstimationPolicy {
        if let Some(exact) = &self.chain.exact {
            return TokenEstimationPolicy::Exact {
                tokenizer: exact.tokenizer.clone(),
            };
        }
        if let Some(approximate) = &self.chain.approximate {
            return TokenEstimationPolicy::Approximate {
                model_family: approximate.model_family.clone(),
                calibration_ratio: approximate.calibration_ratio,
            };
        }
        let chars_per_token = self.chain.heuristic.chars_per_token.max(0.1);
        TokenEstimationPolicy::Heuristic {
            chars_per_token,
            language_hint: self.chain.heuristic.language_hint.clone(),
        }
    }

    fn confidence(&self) -> TokenEstimationConfidence {
        if self.chain.exact.is_some() {
            return TokenEstimationConfidence::Exact;
        }
        if let Some(approximate) = &self.chain.approximate {
            return if approximate.calibrated {
                TokenEstimationConfidence::High
            } else {
                TokenEstimationConfidence::Medium
            };
        }
        TokenEstimationConfidence::Low
    }

    fn safety_margin_pct(&self, margins: &TokenSafetyMargins) -> f32 {
        match self.confidence() {
            TokenEstimationConfidence::Exact => margins.exact_pct,
            TokenEstimationConfidence::High | TokenEstimationConfidence::Medium => {
                margins.approximate_pct
            }
            TokenEstimationConfidence::Low => margins.heuristic_pct,
        }
    }

    fn estimate(&self, text: &str) -> usize {
        if self.chain.exact.is_some() {
            return estimate_exact_tokens(text);
        }
        if let Some(approximate) = &self.chain.approximate {
            return estimate_approximate_tokens(text, approximate.calibration_ratio);
        }
        estimate_heuristic_tokens(text, &self.chain.heuristic)
    }
}

#[derive(Debug, Clone)]
struct EstimatedItem {
    id: String,
    description: String,
    content: String,
    sequence: i64,
    priority_score: Option<f32>,
    is_stale: bool,
    user_excluded: bool,
    token_count: usize,
}

fn estimate_layer_inputs(
    inputs: &[ContextLayerInput],
    estimator: ActiveEstimator<'_>,
) -> BTreeMap<ContextLayerKind, Vec<EstimatedItem>> {
    let mut map: BTreeMap<ContextLayerKind, Vec<EstimatedItem>> = BTreeMap::new();

    for input in inputs {
        let mut estimated_items = input
            .items
            .iter()
            .map(|item| EstimatedItem {
                id: item.id.clone(),
                description: item.description.clone(),
                content: item.content.clone(),
                sequence: item.sequence,
                priority_score: item.priority_score,
                is_stale: item.is_stale,
                user_excluded: item.user_excluded,
                token_count: estimator.estimate(&item.content),
            })
            .collect::<Vec<_>>();
        estimated_items.sort_by(|left, right| {
            left.sequence
                .cmp(&right.sequence)
                .then_with(|| left.id.cmp(&right.id))
        });

        map.entry(input.kind)
            .and_modify(|existing| existing.extend(estimated_items.clone()))
            .or_insert(estimated_items);
    }

    map
}

fn build_hard_layer_plan(layer: &ContextLayer, items: &[EstimatedItem]) -> ContextPlanLayer {
    let content = items
        .iter()
        .map(|item| item.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");

    let token_count = items.iter().map(|item| item.token_count).sum();

    ContextPlanLayer {
        kind: layer.kind,
        content,
        token_count,
        allocated_budget_tokens: 0,
        is_hard_context: true,
        was_truncated: false,
        truncation_reason: None,
        items_included: items.len(),
        items_omitted: 0,
        omitted_items: Vec::new(),
    }
}

fn build_soft_layer_plan(
    layer: &ContextLayer,
    items: &[EstimatedItem],
    allocated_budget_tokens: usize,
    estimator: ActiveEstimator<'_>,
) -> ContextPlanLayer {
    let ordered = order_for_strategy(items, layer.collapse_strategy);

    let mut included = Vec::new();
    let mut omitted_items = Vec::new();
    let mut remaining = allocated_budget_tokens;
    let mut was_truncated = false;
    let mut truncation_reason = None;

    for item in ordered {
        if item.user_excluded {
            omitted_items.push(OmittedItem {
                item_id: item.id.clone(),
                description: item.description.clone(),
                token_count: item.token_count,
                reason: OmissionReason::UserExcluded,
            });
            continue;
        }

        if item.is_stale {
            omitted_items.push(OmittedItem {
                item_id: item.id.clone(),
                description: item.description.clone(),
                token_count: item.token_count,
                reason: OmissionReason::StaleArtifact,
            });
            continue;
        }

        if let Some(score) = item.priority_score {
            if score < 0.0 {
                omitted_items.push(OmittedItem {
                    item_id: item.id.clone(),
                    description: item.description.clone(),
                    token_count: item.token_count,
                    reason: OmissionReason::PriorityTooLow { score },
                });
                continue;
            }
        }

        if item.token_count <= remaining {
            remaining -= item.token_count;
            included.push(item.clone());
            continue;
        }

        if remaining > 0
            && matches!(
                layer.collapse_strategy,
                CollapseStrategy::TruncateTail
                    | CollapseStrategy::TruncateHead
                    | CollapseStrategy::Summarize
            )
        {
            let (truncated_content, truncated_tokens) =
                truncate_item_to_budget(&item, remaining, layer.collapse_strategy, estimator);
            if !truncated_content.is_empty() && truncated_tokens > 0 {
                included.push(EstimatedItem {
                    content: truncated_content,
                    token_count: truncated_tokens,
                    ..item.clone()
                });
                was_truncated = true;
                truncation_reason = Some(format!(
                    "{:?} reduced item {} to fit budget",
                    layer.collapse_strategy, item.id
                ));
            }
            omitted_items.push(OmittedItem {
                item_id: item.id.clone(),
                description: item.description.clone(),
                token_count: item.token_count.saturating_sub(truncated_tokens),
                reason: OmissionReason::BudgetExceeded,
            });
            remaining = 0;
            continue;
        }

        omitted_items.push(OmittedItem {
            item_id: item.id.clone(),
            description: item.description.clone(),
            token_count: item.token_count,
            reason: OmissionReason::BudgetExceeded,
        });
    }

    let token_count = included.iter().map(|item| item.token_count).sum::<usize>();
    let content = included
        .iter()
        .map(|item| item.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");

    ContextPlanLayer {
        kind: layer.kind,
        content,
        token_count,
        allocated_budget_tokens,
        is_hard_context: false,
        was_truncated,
        truncation_reason,
        items_included: included.len(),
        items_omitted: omitted_items
            .iter()
            .filter(|omission| {
                !matches!(omission.reason, OmissionReason::BudgetExceeded)
                    || omission.token_count > 0
            })
            .count(),
        omitted_items,
    }
}

fn order_for_strategy(items: &[EstimatedItem], strategy: CollapseStrategy) -> Vec<EstimatedItem> {
    let mut ordered = items.to_vec();
    match strategy {
        CollapseStrategy::OmitOldest | CollapseStrategy::TruncateHead => {
            ordered.sort_by_key(|right| std::cmp::Reverse(right.sequence));
        }
        _ => {
            ordered.sort_by_key(|left| left.sequence);
        }
    }
    ordered
}

fn truncate_item_to_budget(
    item: &EstimatedItem,
    target_tokens: usize,
    strategy: CollapseStrategy,
    estimator: ActiveEstimator<'_>,
) -> (String, usize) {
    if target_tokens == 0 || item.content.is_empty() {
        return (String::new(), 0);
    }

    let chars = item.content.chars().count();
    let mut keep_chars = (((target_tokens as f32) / (item.token_count.max(1) as f32))
        * (chars as f32))
        .floor() as usize;
    keep_chars = keep_chars.max(1).min(chars);

    let mut candidate = slice_for_strategy(&item.content, keep_chars, strategy);
    if keep_chars < chars {
        candidate.push('…');
    }

    let mut estimated = estimator.estimate(&candidate);
    while estimated > target_tokens && keep_chars > 1 {
        keep_chars -= 1;
        candidate = slice_for_strategy(&item.content, keep_chars, strategy);
        if keep_chars < chars {
            candidate.push('…');
        }
        estimated = estimator.estimate(&candidate);
    }

    (candidate, estimated.min(target_tokens))
}

fn slice_for_strategy(content: &str, keep_chars: usize, strategy: CollapseStrategy) -> String {
    let chars = content.chars().collect::<Vec<_>>();
    if keep_chars >= chars.len() {
        return content.to_string();
    }

    match strategy {
        CollapseStrategy::TruncateHead => chars[chars.len() - keep_chars..].iter().collect(),
        _ => chars[..keep_chars].iter().collect(),
    }
}

fn allocate_layer_budgets(
    layers: &[&ContextLayer],
    budget: usize,
) -> Result<BTreeMap<ContextLayerKind, usize>, ContextAssemblyError> {
    let mut allocations = BTreeMap::new();
    if layers.is_empty() || budget == 0 {
        return Ok(allocations);
    }

    let mut rows = layers
        .iter()
        .map(|layer| BudgetRow {
            kind: layer.kind,
            priority: layer.priority,
            min_tokens: pct_of_tokens(budget, layer.min_budget_pct),
            flex_tokens: pct_of_tokens(
                budget,
                (layer.max_budget_pct - layer.min_budget_pct).max(0.0),
            ),
            assigned_tokens: 0,
            fractional_weight: 0.0,
        })
        .collect::<Vec<_>>();

    let min_total = rows.iter().map(|row| row.min_tokens).sum::<usize>();
    if min_total > budget {
        return Err(ContextAssemblyError::InvalidPolicy {
            reason: format!(
                "minimum allocations require {min_total} tokens but budget is {budget}"
            ),
        });
    }

    for row in &mut rows {
        row.assigned_tokens = row.min_tokens;
    }

    let remaining = budget - min_total;
    let flex_total = rows.iter().map(|row| row.flex_tokens).sum::<usize>();

    if remaining > 0 && flex_total > 0 {
        let mut distributed = 0usize;
        for row in &mut rows {
            let weighted = (remaining as f64) * (row.flex_tokens as f64 / flex_total as f64);
            let base = weighted.floor() as usize;
            row.assigned_tokens += base;
            row.fractional_weight = (weighted - weighted.floor()) as f32;
            distributed += base;
        }

        let mut leftover = remaining.saturating_sub(distributed);
        rows.sort_by(|left, right| {
            right
                .fractional_weight
                .partial_cmp(&left.fractional_weight)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.priority.cmp(&right.priority))
                .then_with(|| left.kind.cmp(&right.kind))
        });

        for row in &mut rows {
            if leftover == 0 {
                break;
            }
            row.assigned_tokens += 1;
            leftover -= 1;
        }
    }

    for row in rows {
        allocations.insert(row.kind, row.assigned_tokens);
    }

    Ok(allocations)
}

#[derive(Debug, Clone)]
struct BudgetRow {
    kind: ContextLayerKind,
    priority: u8,
    min_tokens: usize,
    flex_tokens: usize,
    assigned_tokens: usize,
    fractional_weight: f32,
}

fn estimate_exact_tokens(text: &str) -> usize {
    let mut count = 0usize;
    let mut in_word = false;

    for ch in text.chars() {
        if ch.is_alphanumeric() {
            if !in_word {
                count += 1;
                in_word = true;
            }
        } else if ch.is_whitespace() {
            in_word = false;
        } else {
            count += 1;
            in_word = false;
        }
    }

    count.max(1)
}

fn estimate_approximate_tokens(text: &str, calibration_ratio: f32) -> usize {
    let ratio = calibration_ratio.max(0.25);
    ((text.chars().count() as f32) / ratio).ceil() as usize
}

fn estimate_heuristic_tokens(text: &str, heuristic: &HeuristicEstimator) -> usize {
    let family_ratio = match detect_language_family(text) {
        LanguageFamily::Latin => 4.0,
        LanguageFamily::Cjk => 1.5,
        LanguageFamily::Cyrillic => 3.0,
        LanguageFamily::ArabicHebrew => 2.5,
        LanguageFamily::MixedUnknown => 3.0,
    };
    let ratio = if heuristic.chars_per_token > 0.0 {
        heuristic.chars_per_token
    } else {
        family_ratio
    };
    ((text.chars().count() as f32) / ratio.max(0.25)).ceil() as usize
}

#[derive(Debug, Clone, Copy)]
enum LanguageFamily {
    Latin,
    Cjk,
    Cyrillic,
    ArabicHebrew,
    MixedUnknown,
}

fn detect_language_family(text: &str) -> LanguageFamily {
    let mut latin = 0usize;
    let mut cjk = 0usize;
    let mut cyrillic = 0usize;
    let mut arabic_hebrew = 0usize;

    for ch in text.chars() {
        let code = ch as u32;
        match code {
            0x0041..=0x024F => latin += 1,
            0x0400..=0x052F => cyrillic += 1,
            0x0590..=0x08FF => arabic_hebrew += 1,
            0x2E80..=0x9FFF | 0xF900..=0xFAFF => cjk += 1,
            _ => {}
        }
    }

    let max = latin.max(cjk).max(cyrillic).max(arabic_hebrew);
    if max == 0 {
        return LanguageFamily::MixedUnknown;
    }

    if cjk == max {
        return LanguageFamily::Cjk;
    }
    if cyrillic == max {
        return LanguageFamily::Cyrillic;
    }
    if arabic_hebrew == max {
        return LanguageFamily::ArabicHebrew;
    }

    if latin == max {
        return LanguageFamily::Latin;
    }

    LanguageFamily::MixedUnknown
}

fn pct_of_tokens(total: usize, pct: f32) -> usize {
    ((total as f32) * (pct.clamp(0.0, 100.0) / 100.0)).floor() as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use ozone_core::{engine::MessageId, session::SessionId};

    fn sample_policy() -> ContextLayerPolicy {
        ContextLayerPolicy {
            layers: vec![
                ContextLayer {
                    kind: ContextLayerKind::SystemPrompt,
                    priority: 0,
                    min_budget_pct: 10.0,
                    max_budget_pct: 20.0,
                    is_hard_context: true,
                    collapse_strategy: CollapseStrategy::Never,
                },
                ContextLayer {
                    kind: ContextLayerKind::RecentMessages,
                    priority: 1,
                    min_budget_pct: 20.0,
                    max_budget_pct: 80.0,
                    is_hard_context: false,
                    collapse_strategy: CollapseStrategy::OmitOldest,
                },
            ],
        }
    }

    fn approximate_chain() -> TokenEstimatorChain {
        TokenEstimatorChain {
            exact: None,
            approximate: Some(ApproximateEstimator {
                model_family: "llama".to_string(),
                calibration_ratio: 4.0,
                calibrated: true,
            }),
            heuristic: HeuristicEstimator::default(),
        }
    }

    #[test]
    fn budget_invariants_hold_for_successful_plan() {
        let assembler = ContextAssembler::new(
            sample_policy(),
            approximate_chain(),
            TokenSafetyMargins {
                exact_pct: 0.0,
                approximate_pct: 0.1,
                heuristic_pct: 0.2,
            },
        );

        let request = ContextPlanRequest {
            budget_tokens: 120,
            created_at: 42,
            is_dry_run: true,
            plan_id: Some("plan-budget".to_string()),
            safety_margin_tokens: Some(12),
            layers: vec![
                ContextLayerInput {
                    kind: ContextLayerKind::SystemPrompt,
                    items: vec![ContextItemInput {
                        id: "sys-1".to_string(),
                        description: "system".to_string(),
                        content: "You are a helpful assistant.".to_string(),
                        sequence: 0,
                        priority_score: None,
                        is_stale: false,
                        user_excluded: false,
                    }],
                },
                ContextLayerInput {
                    kind: ContextLayerKind::RecentMessages,
                    items: vec![
                        ContextItemInput {
                            id: "m1".to_string(),
                            description: "old".to_string(),
                            content: "user: hello there".to_string(),
                            sequence: 1,
                            priority_score: None,
                            is_stale: false,
                            user_excluded: false,
                        },
                        ContextItemInput {
                            id: "m2".to_string(),
                            description: "new".to_string(),
                            content: "assistant: hi! how can I help today?".to_string(),
                            sequence: 2,
                            priority_score: None,
                            is_stale: false,
                            user_excluded: false,
                        },
                    ],
                },
            ],
        };

        let plan = assembler.build_plan(request).unwrap();
        assert!(plan.total_tokens <= plan.budget.saturating_sub(plan.safety_margin_tokens));
        assert!(plan.total_tokens <= plan.budget);

        let hard_layer = plan
            .layers
            .iter()
            .find(|layer| layer.kind == ContextLayerKind::SystemPrompt)
            .unwrap();
        assert!(hard_layer.token_count > 0);
        assert_eq!(hard_layer.items_omitted, 0);
    }

    #[test]
    fn hard_context_is_kept_while_soft_context_is_omitted() {
        let assembler = ContextAssembler::new(
            sample_policy(),
            approximate_chain(),
            TokenSafetyMargins::default(),
        );

        let request = ContextPlanRequest {
            budget_tokens: 60,
            created_at: 43,
            is_dry_run: true,
            plan_id: Some("plan-hard-soft".to_string()),
            safety_margin_tokens: Some(0),
            layers: vec![
                ContextLayerInput {
                    kind: ContextLayerKind::SystemPrompt,
                    items: vec![ContextItemInput {
                        id: "sys".to_string(),
                        description: "system".to_string(),
                        content: "SYSTEM ".repeat(30),
                        sequence: 0,
                        priority_score: None,
                        is_stale: false,
                        user_excluded: false,
                    }],
                },
                ContextLayerInput {
                    kind: ContextLayerKind::RecentMessages,
                    items: vec![ContextItemInput {
                        id: "recent".to_string(),
                        description: "recent".to_string(),
                        content: "user message ".repeat(30),
                        sequence: 1,
                        priority_score: None,
                        is_stale: false,
                        user_excluded: false,
                    }],
                },
            ],
        };

        let plan = assembler.build_plan(request).unwrap();
        let hard = plan
            .layers
            .iter()
            .find(|layer| layer.kind == ContextLayerKind::SystemPrompt)
            .unwrap();
        let soft = plan
            .layers
            .iter()
            .find(|layer| layer.kind == ContextLayerKind::RecentMessages)
            .unwrap();

        assert!(hard.token_count > hard.allocated_budget_tokens);
        assert_eq!(hard.items_included, 1);
        assert_eq!(soft.items_included, 0);
        assert_eq!(soft.items_omitted, 1);
        assert!(matches!(
            soft.omitted_items[0].reason,
            OmissionReason::BudgetExceeded
        ));
    }

    #[test]
    fn output_shape_is_stable_and_inspectable() {
        let assembler = ContextAssembler::new(
            sample_policy(),
            approximate_chain(),
            TokenSafetyMargins::default(),
        );

        let session = SessionId::parse("00000000-0000-4000-8000-000000000001").unwrap();
        let msg = ConversationMessage::new(
            session,
            MessageId::parse("10000000-0000-4000-8000-000000000001").unwrap(),
            "user",
            "hello",
            10,
        );

        let transcript_request = TranscriptPlanRequest {
            budget_tokens: 100,
            created_at: 99,
            is_dry_run: true,
            plan_id: Some("plan-stable".to_string()),
            safety_margin_tokens: Some(5),
            prepend_layers: vec![ContextLayerInput {
                kind: ContextLayerKind::SystemPrompt,
                items: vec![ContextItemInput {
                    id: "sys".to_string(),
                    description: "system".to_string(),
                    content: "You are concise.".to_string(),
                    sequence: 0,
                    priority_score: None,
                    is_stale: false,
                    user_excluded: false,
                }],
            }],
        };

        let plan = assembler
            .build_plan_from_transcript(&[msg], transcript_request)
            .unwrap();

        assert_eq!(plan.id, "plan-stable");
        assert_eq!(plan.created_at, 99);
        assert_eq!(plan.layers.len(), 2);
        assert_eq!(plan.layers[0].kind, ContextLayerKind::SystemPrompt);
        assert_eq!(plan.layers[1].kind, ContextLayerKind::RecentMessages);
        assert_eq!(plan.layers[1].items_included, 1);
        assert!(plan.layers[1].content.contains("user: hello"));
    }
}
