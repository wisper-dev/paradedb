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

//! Range and term queries with bounds or values at or beyond the edges of an integer field's
//! range return the same rows as the equivalent plain SQL.
//! Timestamps are stored as i64, with '-infinity' and 'infinity' at the edges.

use pretty_assertions::assert_eq;
use rstest::*;
use sqlx::PgConnection;
use tests::fixtures::*;

/// Checks that each pg_search condition selects the same rows of `table` as the plain SQL
/// condition selects of `{table}_plain`, an unindexed copy, and that there are `expected` of them.
fn assert_same_rows(conn: &mut PgConnection, table: &str, cases: &[(&str, &str, usize)]) {
    for &(search, plain, expected) in cases {
        let rows: Vec<(i32,)> =
            format!("SELECT id FROM {table} WHERE {search} ORDER BY id").fetch(conn);
        let plain_rows: Vec<(i32,)> =
            format!("SELECT id FROM {table}_plain WHERE {plain} ORDER BY id").fetch(conn);
        assert_eq!(rows, plain_rows, "{search}");
        assert_eq!(rows.len(), expected, "{search}");
    }
}

#[rstest]
fn bigint_bounds(mut conn: PgConnection) {
    // -446744073709551616 is what 18000000000000000000 becomes if it wraps around as a bigint.
    r#"
    CREATE TABLE bigints (id SERIAL PRIMARY KEY, x BIGINT);
    INSERT INTO bigints (x) SELECT generate_series(1, 1000);
    INSERT INTO bigints (x) VALUES
        (-9223372036854775808), (-446744073709551616), (9223372036854775807);
    CREATE INDEX bigints_idx ON bigints USING paradedb (id, x);
    CREATE TABLE bigints_plain AS TABLE bigints;
    "#
    .execute(&mut conn);

    assert_same_rows(
        &mut conn,
        "bigints",
        &[
            // JSON bounds at and beyond the maximum.
            (
                r#"id @@@ '{"range":{"field":"x","lower_bound":{"included":9223372036854775807},"upper_bound":null}}'::jsonb"#,
                "x >= 9223372036854775807",
                1,
            ),
            (
                r#"id @@@ '{"range":{"field":"x","lower_bound":{"included":9223372036854775808},"upper_bound":null}}'::jsonb"#,
                "x >= 9223372036854775808",
                0,
            ),
            (
                r#"id @@@ '{"range":{"field":"x","lower_bound":{"excluded":9223372036854775807},"upper_bound":null}}'::jsonb"#,
                "x > 9223372036854775807",
                0,
            ),
            (
                r#"id @@@ '{"range":{"field":"x","lower_bound":null,"upper_bound":{"included":9223372036854775807}}}'::jsonb"#,
                "x <= 9223372036854775807",
                1003,
            ),
            (
                r#"id @@@ '{"range":{"field":"x","lower_bound":null,"upper_bound":{"included":18000000000000000000}}}'::jsonb"#,
                "x <= 18000000000000000000",
                1003,
            ),
            (
                r#"id @@@ '{"range":{"field":"x","lower_bound":{"excluded":500},"upper_bound":{"included":600}}}'::jsonb"#,
                "x > 500 AND x <= 600",
                100,
            ),
            // pdb.range bounds at and beyond either edge.
            (
                "x @@@ pdb.range(numrange(NULL, 18000000000000000000, '[]'))",
                "x <= 18000000000000000000",
                1003,
            ),
            (
                "x @@@ pdb.range(numrange(NULL, 9223372036854775807, '[]'))",
                "x <= 9223372036854775807",
                1003,
            ),
            (
                "x @@@ pdb.range(numrange(9223372036854775807, NULL, '()'))",
                "x > 9223372036854775807",
                0,
            ),
            (
                "x @@@ pdb.range(numrange(9223372036854775807, NULL, '[]'))",
                "x >= 9223372036854775807",
                1,
            ),
            (
                "x @@@ pdb.range(numrange(18000000000000000000, NULL, '[]'))",
                "x >= 18000000000000000000",
                0,
            ),
            (
                "x @@@ pdb.range(numrange(NULL, -9223372036854775809, '[]'))",
                "x <= -9223372036854775809",
                0,
            ),
            (
                "x @@@ pdb.range(numrange(-9223372036854775809, NULL, '()'))",
                "x > -9223372036854775809",
                1003,
            ),
            (
                "x @@@ pdb.range(numrange(500, 600, '(]'))",
                "x > 500 AND x <= 600",
                100,
            ),
            // Terms beyond either edge match nothing.
            (
                r#"id @@@ '{"term":{"field":"x","value":18000000000000000000}}'::jsonb"#,
                "x = 18000000000000000000",
                0,
            ),
            (
                r#"id @@@ '{"term_set":{"terms":[{"field":"x","value":18000000000000000000},{"field":"x","value":5}]}}'::jsonb"#,
                "x IN (18000000000000000000, 5)",
                1,
            ),
            (
                "x @@@ pdb.term(18000000000000000000::numeric)",
                "x = 18000000000000000000",
                0,
            ),
            (
                "x @@@ pdb.term(-9223372036854775809::numeric)",
                "x = -9223372036854775809",
                0,
            ),
            (
                "x @@@ pdb.term_set(ARRAY[-9223372036854775809, 5, 18000000000000000000]::numeric[])",
                "x IN (-9223372036854775809, 5, 18000000000000000000)",
                1,
            ),
            // A bigint constant at the maximum is pushed down as a range.
            (
                "id @@@ pdb.all() AND x > 9223372036854775807::bigint",
                "x > 9223372036854775807::bigint",
                0,
            ),
            (
                "id @@@ pdb.all() AND x <= 9223372036854775807::bigint",
                "x <= 9223372036854775807::bigint",
                1003,
            ),
        ],
    );
}

#[rstest]
fn oid_bounds(mut conn: PgConnection) {
    // An oid field holds values from 0 to 18446744073709551615 in the index.
    r#"
    CREATE TABLE oids (id SERIAL PRIMARY KEY, o OID);
    INSERT INTO oids (o) VALUES (1), (5), (4294967295);
    CREATE INDEX oids_idx ON oids USING paradedb (id, o);
    CREATE TABLE oids_plain AS TABLE oids;
    "#
    .execute(&mut conn);

    assert_same_rows(
        &mut conn,
        "oids",
        &[
            (
                r#"id @@@ '{"range":{"field":"o","lower_bound":null,"upper_bound":{"included":18446744073709551615}}}'::jsonb"#,
                "o::bigint <= 18446744073709551615",
                3,
            ),
            (
                r#"id @@@ '{"range":{"field":"o","lower_bound":{"excluded":18446744073709551615},"upper_bound":null}}'::jsonb"#,
                "o::bigint > 18446744073709551615",
                0,
            ),
            (
                "o @@@ pdb.range(numrange(-5, NULL, '[]'))",
                "o::bigint >= -5",
                3,
            ),
        ],
    );
}

#[rstest]
fn timestamp_infinity_bounds(mut conn: PgConnection) {
    r#"
    CREATE TABLE timestamps (id SERIAL PRIMARY KEY, t TIMESTAMP);
    INSERT INTO timestamps (t) VALUES ('-infinity'), ('2024-01-01'), ('infinity');
    CREATE INDEX timestamps_idx ON timestamps USING paradedb (id, t);
    CREATE TABLE timestamps_plain AS TABLE timestamps;
    "#
    .execute(&mut conn);

    assert_same_rows(
        &mut conn,
        "timestamps",
        &[
            (
                r#"id @@@ '{"range":{"field":"t","lower_bound":{"excluded":"infinity"},"upper_bound":null}}'::jsonb"#,
                "t > 'infinity'",
                0,
            ),
            (
                r#"id @@@ '{"range":{"field":"t","lower_bound":null,"upper_bound":{"included":"infinity"}}}'::jsonb"#,
                "t <= 'infinity'",
                3,
            ),
        ],
    );
}

#[rstest]
fn segment_pruning_with_a_term_beyond_the_maximum(mut conn: PgConnection) {
    // Each insert writes its own segment, so the maximum sits alone in one.
    r#"
    CREATE TABLE pruning (id SERIAL PRIMARY KEY, x BIGINT NOT NULL);
    CREATE INDEX pruning_idx ON pruning USING paradedb (id, x)
    WITH (partition_by = 'x', background_layer_sizes = '0');
    SET paradedb.global_mutable_segment_rows = 0;
    INSERT INTO pruning (x) VALUES (5);
    INSERT INTO pruning (x) VALUES (9223372036854775807);
    CREATE TABLE pruning_plain AS TABLE pruning;
    "#
    .execute(&mut conn);

    assert_same_rows(
        &mut conn,
        "pruning",
        &[(
            "id @@@ pdb.all() AND NOT x @@@ pdb.term(18000000000000000000::numeric)",
            "NOT x = 18000000000000000000",
            2,
        )],
    );
}
