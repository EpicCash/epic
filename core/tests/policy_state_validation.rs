// Copyright 2026 The Epic Cash Developers
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use epic_core::core::block::feijoada::{
	get_bottles_default, next_block_bottles, validate_bottles, Deterministic, Feijoada,
	FeijoadaError, Policy, PolicyConfig,
};
use epic_core::consensus;
use epic_core::pow::PoWType;
use std::collections::HashMap;

#[test]
fn missing_scheduled_bottle_returns_error() {
	let mut policy = HashMap::new();
	policy.insert(PoWType::RandomX, 100);
	let bottles = HashMap::new();

	assert_eq!(
		Err(FeijoadaError::MissingBottle(PoWType::RandomX)),
		Deterministic::choose_algo(&policy, &bottles)
	);
}

#[test]
fn empty_policy_and_overflowing_bottles_return_errors() {
	let mut bottles = HashMap::new();
	bottles.insert(PoWType::RandomX, u32::MAX);
	bottles.insert(PoWType::ProgPow, 1);

	assert_eq!(
		Err(FeijoadaError::NoScheduledAlgorithm),
		Deterministic::choose_algo(&HashMap::new(), &bottles)
	);

	let mut policy = HashMap::new();
	policy.insert(PoWType::RandomX, 50);
	policy.insert(PoWType::ProgPow, 50);
	assert_eq!(
		Err(FeijoadaError::BeanCountOverflow),
		Deterministic::choose_algo(&policy, &bottles)
	);
}

#[test]
fn next_policy_propagates_invalid_stored_bottles() {
	let bottles = HashMap::new();
	assert!(matches!(
		consensus::next_policy(0, vec![bottles]),
		Err(FeijoadaError::MissingBottle(_))
	));
}

#[test]
fn bottle_totals_above_reset_boundary_are_rejected() {
	let policy = PolicyConfig::default().policies.pop().unwrap();
	for count in [101, u32::MAX] {
		let mut bottles = get_bottles_default();
		bottles.insert(PoWType::RandomX, count);
		assert_eq!(
			Err(FeijoadaError::BeanCountOverflow),
			validate_bottles(&policy, &bottles)
		);
	}

	let mut bottles = get_bottles_default();
	bottles.insert(PoWType::RandomX, 100);
	assert_eq!(Ok(()), validate_bottles(&policy, &bottles));
}

fn legacy_choose_algo(policy: &Policy, bottles: &Policy) -> PoWType {
	let bean_total = std::cmp::max(bottles.values().sum::<u32>(), 1);
	let mut policy_vec: Vec<(PoWType, f32)> = policy
		.iter()
		.filter_map(|(&algo, &proportion)| (proportion > 0).then_some((algo, proportion as f32)))
		.collect();
	policy_vec.sort_by(|(left, _), (right, _)| left.cmp(right));
	let scores: HashMap<PoWType, f32> = bottles
		.iter()
		.map(|(&algo, &beans)| (algo, 100.0 * beans as f32 / bean_total as f32))
		.collect();
	policy_vec
		.iter()
		.map(|(algo, proportion)| (algo, proportion - scores[algo]))
		.max_by(|(_, left), (_, right)| left.partial_cmp(right).unwrap())
		.map(|(algo, _)| *algo)
		.unwrap()
}

#[test]
fn valid_policy_selection_is_unchanged() {
	for policy in PolicyConfig::default().policies {
		let mut bottles = get_bottles_default();
		for _ in 0..=100 {
			let expected = legacy_choose_algo(&policy, &bottles);
			assert_eq!(expected, Deterministic::choose_algo(&policy, &bottles).unwrap());
			bottles = next_block_bottles(expected, &bottles).unwrap();
		}
	}
}
