// Copyright (c) 2023-2026 ParadeDB, Inc.
//
// This file is part of ParadeDB - Postgres for Search and Analytics
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <http://www.gnu.org/licenses/>.

use crate::api::version::Version;
use crate::postgres::pdb_owned_value::PdbOwnedValue;
use crate::query::value_to_json_term;
use crate::schema::IndexRecordOption;
use anyhow::Result;
use serde::de::Error as SerdeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::ops::Bound;
use tantivy::{
    Term,
    query::{RangeQuery, RegexQuery, TermQuery},
    schema::Field,
};

const EMPTY_KEY: &str = "empty";
const LOWER_KEY: &str = "lower";
const UPPER_KEY: &str = "upper";
const LOWER_INCLUSIVE_KEY: &str = "lower_inclusive";
const UPPER_INCLUSIVE_KEY: &str = "upper_inclusive";
const LOWER_UNBOUNDED_KEY: &str = "lower_unbounded";
const UPPER_UNBOUNDED_KEY: &str = "upper_unbounded";
// Always false for range fields
const EXPAND_DOTS: bool = false;
// Range term queries are boolean exact-match predicates and only require docids.
const RECORD: IndexRecordOption = IndexRecordOption::Basic;

#[derive(Clone, Debug)]
pub struct RangeField {
    field: Field,
}

#[derive(Debug, PartialEq)]
pub enum Comparison {
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

impl RangeField {
    pub fn new(field: Field) -> Self {
        Self { field }
    }

    pub fn exists(&self) -> Result<RegexQuery> {
        Ok(RegexQuery::from_pattern(".*", self.field)?)
    }

    pub fn empty(&self, val: bool) -> Result<TermQuery> {
        let term = Self::as_range_term(self, &PdbOwnedValue::Bool(val), Some(EMPTY_KEY), None)?;
        Ok(TermQuery::new(term, RECORD.into()))
    }

    pub fn upper_bound_inclusive(&self, val: bool) -> Result<TermQuery> {
        let term = Self::as_range_term(
            self,
            &PdbOwnedValue::Bool(val),
            Some(UPPER_INCLUSIVE_KEY),
            None,
        )?;
        Ok(TermQuery::new(term, RECORD.into()))
    }

    pub fn lower_bound_inclusive(&self, val: bool) -> Result<TermQuery> {
        let term = Self::as_range_term(
            self,
            &PdbOwnedValue::Bool(val),
            Some(LOWER_INCLUSIVE_KEY),
            None,
        )?;
        Ok(TermQuery::new(term, RECORD.into()))
    }

    pub fn upper_bound_unbounded(&self, val: bool) -> Result<TermQuery> {
        let term = Self::as_range_term(
            self,
            &PdbOwnedValue::Bool(val),
            Some(UPPER_UNBOUNDED_KEY),
            None,
        )?;
        Ok(TermQuery::new(term, RECORD.into()))
    }

    pub fn lower_bound_unbounded(&self, val: bool) -> Result<TermQuery> {
        let term = Self::as_range_term(
            self,
            &PdbOwnedValue::Bool(val),
            Some(LOWER_UNBOUNDED_KEY),
            None,
        )?;
        Ok(TermQuery::new(term, RECORD.into()))
    }

    pub fn compare_lower_bound(
        &self,
        owned: &PdbOwnedValue,
        comparison: Comparison,
        index_created_by_version: Option<Version>,
    ) -> Result<RangeQuery> {
        let (owned, comparison) = clamp_to_i64(owned, comparison);
        let query = match comparison {
            Comparison::LessThan => RangeQuery::new(
                Bound::Excluded(Self::as_range_term(
                    self,
                    owned,
                    Some(LOWER_KEY),
                    index_created_by_version,
                )?),
                Bound::Unbounded,
            ),
            Comparison::LessThanOrEqual => RangeQuery::new(
                Bound::Included(Self::as_range_term(
                    self,
                    owned,
                    Some(LOWER_KEY),
                    index_created_by_version,
                )?),
                Bound::Unbounded,
            ),
            Comparison::GreaterThan => RangeQuery::new(
                Bound::Unbounded,
                Bound::Excluded(Self::as_range_term(
                    self,
                    owned,
                    Some(LOWER_KEY),
                    index_created_by_version,
                )?),
            ),
            Comparison::GreaterThanOrEqual => RangeQuery::new(
                Bound::Unbounded,
                Bound::Included(Self::as_range_term(
                    self,
                    owned,
                    Some(LOWER_KEY),
                    index_created_by_version,
                )?),
            ),
        };

        Ok(query)
    }

    pub fn compare_upper_bound(
        &self,
        owned: &PdbOwnedValue,
        comparison: Comparison,
        index_created_by_version: Option<Version>,
    ) -> Result<RangeQuery> {
        let (owned, comparison) = clamp_to_i64(owned, comparison);
        let query = match comparison {
            Comparison::LessThan => RangeQuery::new(
                Bound::Excluded(Self::as_range_term(
                    self,
                    owned,
                    Some(UPPER_KEY),
                    index_created_by_version,
                )?),
                Bound::Unbounded,
            ),
            Comparison::LessThanOrEqual => RangeQuery::new(
                Bound::Included(Self::as_range_term(
                    self,
                    owned,
                    Some(UPPER_KEY),
                    index_created_by_version,
                )?),
                Bound::Unbounded,
            ),
            Comparison::GreaterThan => RangeQuery::new(
                Bound::Unbounded,
                Bound::Excluded(Self::as_range_term(
                    self,
                    owned,
                    Some(UPPER_KEY),
                    index_created_by_version,
                )?),
            ),
            Comparison::GreaterThanOrEqual => RangeQuery::new(
                Bound::Unbounded,
                Bound::Included(Self::as_range_term(
                    self,
                    owned,
                    Some(UPPER_KEY),
                    index_created_by_version,
                )?),
            ),
        };

        Ok(query)
    }

    fn as_range_term(
        &self,
        value: &PdbOwnedValue,
        path: Option<&str>,
        index_created_by_version: Option<Version>,
    ) -> Result<Term> {
        value_to_json_term(
            self.field,
            value,
            path,
            EXPAND_DOTS,
            index_created_by_version,
        )
    }
}

/// Range fields store every number as an `i64`. A number beyond the `i64` range lies above or
/// below every stored bound, so it compares with each bound like the nearest `i64` under a strict
/// or a non-strict comparison.
fn clamp_to_i64(value: &PdbOwnedValue, comparison: Comparison) -> (&PdbOwnedValue, Comparison) {
    use Comparison::*;
    static MIN: PdbOwnedValue = PdbOwnedValue::I64(i64::MIN);
    static MAX: PdbOwnedValue = PdbOwnedValue::I64(i64::MAX);

    let above = match *value {
        PdbOwnedValue::U64(n) if n > i64::MAX as u64 => true,
        PdbOwnedValue::F64(f) if f >= -(i64::MIN as f64) => true,
        PdbOwnedValue::F64(f) if f < i64::MIN as f64 => false,
        _ => return (value, comparison),
    };
    // A value above every stored bound is greater than each of them, as `i64::MAX` is under
    // `GreaterThanOrEqual`, and less than none, as `i64::MAX` is under `LessThan`. A value below
    // every stored bound is the mirror image, with `i64::MIN`.
    match (above, comparison) {
        (true, LessThan | LessThanOrEqual) => (&MAX, LessThan),
        (true, GreaterThan | GreaterThanOrEqual) => (&MAX, GreaterThanOrEqual),
        (false, LessThan | LessThanOrEqual) => (&MIN, LessThanOrEqual),
        (false, GreaterThan | GreaterThanOrEqual) => (&MIN, GreaterThan),
    }
}

/// Custom serialization function for `Bound<T>`.
/// The goal of this function is to serialize `Bound<T>` with **lowercase keys**.
/// By default, Rust would serialize the `Bound` enum using its variant names,
/// but we want to control the output format to ensure that keys like "included",
/// "excluded", and "unbounded" appear in lowercase.
pub fn serialize_bound<S, T>(bound: &Bound<T>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Serialize,
{
    match bound {
        Bound::Included(val) => {
            #[derive(Serialize)]
            #[serde(rename_all = "snake_case")]
            struct IncludedBound<T> {
                included: T,
            }
            IncludedBound { included: val }.serialize(serializer)
        }
        Bound::Excluded(val) => {
            #[derive(Serialize)]
            #[serde(rename_all = "snake_case")]
            struct ExcludedBound<T> {
                excluded: T,
            }
            ExcludedBound { excluded: val }.serialize(serializer)
        }
        Bound::Unbounded => {
            #[derive(Serialize)]
            #[serde(rename_all = "snake_case")]
            struct UnboundedBound;

            UnboundedBound.serialize(serializer)
        }
    }
}

/// Custom deserialization function for `Bound<T>`.
/// This function attempts to deserialize `Bound<T>` with lowercase keys (e.g., "included", "excluded"),
/// and if that fails, it falls back to deserializing with capitalized keys ("Included", "Excluded").
pub fn deserialize_bound<'de, D, T>(deserializer: D) -> Result<Bound<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    // First, deserialize into a `serde_json::Value`.
    let value: Value = Value::deserialize(deserializer)?;

    // Try to deserialize using lowercase keys.
    if let Ok(bound) = LowercaseBoundDef::deserialize(value.clone()) {
        return match bound {
            LowercaseBoundDef::Included { included } => Ok(Bound::Included(included)),
            LowercaseBoundDef::Excluded { excluded } => Ok(Bound::Excluded(excluded)),
            LowercaseBoundDef::Unbounded => Ok(Bound::Unbounded),
        };
    }

    // If lowercase deserialization fails, try with capitalized keys.
    let bound = CapitalizedBoundDef::deserialize(value)
        .map_err(|e| D::Error::custom(format!("Failed to deserialize: {e}")))?; // Convert serde_json error to D::Error

    match bound {
        CapitalizedBoundDef::Included { Included } => Ok(Bound::Included(Included)),
        CapitalizedBoundDef::Excluded { Excluded } => Ok(Bound::Excluded(Excluded)),
        CapitalizedBoundDef::Unbounded => Ok(Bound::Unbounded),
    }
}

// Define Lowercase and Capitalized variants to support both cases.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(untagged)]
enum LowercaseBoundDef<T> {
    Included { included: T },
    Excluded { excluded: T },
    Unbounded,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
#[serde(untagged)]
#[allow(non_snake_case)]
enum CapitalizedBoundDef<T> {
    Included { Included: T },
    Excluded { Excluded: T },
    Unbounded,
}

#[cfg(test)]
mod tests {
    use super::*;
    use Comparison::*;
    use PdbOwnedValue::{F64, I64, U64};
    use rstest::rstest;

    #[rstest]
    #[case::u64_above(U64(1 << 63), LessThanOrEqual, i64::MAX, LessThan)]
    #[case::f64_above(F64(-(i64::MIN as f64)), GreaterThan, i64::MAX, GreaterThanOrEqual)]
    #[case::infinity(F64(f64::INFINITY), LessThan, i64::MAX, LessThan)]
    #[case::f64_below(F64(-1e19), LessThan, i64::MIN, LessThanOrEqual)]
    #[case::negative_infinity(F64(f64::NEG_INFINITY), GreaterThanOrEqual, i64::MIN, GreaterThan)]
    fn test_clamp_to_i64_beyond(
        #[case] value: PdbOwnedValue,
        #[case] comparison: Comparison,
        #[case] edge: i64,
        #[case] expected: Comparison,
    ) {
        assert_eq!(clamp_to_i64(&value, comparison), (&I64(edge), expected));
    }

    #[rstest]
    #[case::u64_at_max(U64(i64::MAX as u64))]
    #[case::f64_at_min(F64(i64::MIN as f64))]
    #[case::i64(I64(5))]
    #[case::string(PdbOwnedValue::Str("18000000000000000000".into()))]
    fn test_clamp_to_i64_inside(#[case] value: PdbOwnedValue) {
        assert_eq!(clamp_to_i64(&value, LessThan), (&value, LessThan));
    }
}
