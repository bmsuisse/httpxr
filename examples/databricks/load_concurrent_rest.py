"""
Databricks Example: Concurrent REST API ingestion → Delta Lake.

This example shows how to use gather_raw_bytes() + iter_json_bytes() for
maximum throughput when fetching from REST APIs with known IDs or NDJSON
endpoints.

Use-cases:
  - Fetch thousands of entities by ID concurrently (product catalog, customers)
  - Stream NDJSON export endpoints into Spark without buffering

Requirements (Databricks cluster):
  pip install httpxr
  pip install orjson  # optional but faster than json
"""

from __future__ import annotations

import json
from typing import Any

import httpxr
from httpxr.extensions import OAuth2Auth, gather_raw_bytes, iter_json_bytes

try:
    import orjson  # type: ignore[import-untyped]

    _parse = orjson.loads
except ImportError:
    _parse = json.loads  # type: ignore[assignment]


# ---------------------------------------------------------------------------
# Concurrent entity fetch (gather_raw_bytes)
# ---------------------------------------------------------------------------


def load_entities_by_id(
    spark: Any,
    base_url: str,
    entity_ids: list[str],
    delta_path: str,
    token: str,
    *,
    max_concurrency: int = 50,
    batch_size: int = 1000,
) -> int:
    """Fetch thousands of entities concurrently and write to Delta.

    Memory-efficient: processes ``batch_size`` entities at a time.

    Parameters
    ----------
    spark:
        Active SparkSession.
    base_url:
        Base URL, e.g. ``"https://api.example.com/entities"``.
    entity_ids:
        List of entity IDs to fetch.
    delta_path:
        Target Delta table path on DBFS.
    token:
        Bearer token for authentication.
    max_concurrency:
        Max simultaneous HTTP requests (default 50).
    batch_size:
        Number of records per Spark write.
    """
    headers = {"Authorization": f"Bearer {token}"}
    total = 0

    with httpxr.Client(headers=headers) as client:
        for batch_start in range(0, len(entity_ids), batch_size):
            batch_ids = entity_ids[batch_start : batch_start + batch_size]
            requests = [
                client.build_request("GET", f"{base_url}/{eid}") for eid in batch_ids
            ]

            # All requests in this batch dispatched concurrently in Rust
            records: list[Any] = gather_raw_bytes(
                client,
                requests,
                max_concurrency=max_concurrency,
                return_exceptions=True,
                parser=_parse,
            )

            good = [r for r in records if not isinstance(r, Exception)]
            errors = [r for r in records if isinstance(r, Exception)]
            if errors:
                print(f"  ⚠ {len(errors)} fetch errors in batch {batch_start}")

            if good:
                df = spark.createDataFrame(good)
                df.write.format("delta").mode("append").save(delta_path)
                total += len(good)
                print(f"  Written {total} records so far…")

    print(f"✓ Loaded {total} entities to {delta_path}")
    return total


# ---------------------------------------------------------------------------
# NDJSON streaming (iter_json_bytes)
# ---------------------------------------------------------------------------


def stream_ndjson_to_delta(
    spark: Any,
    ndjson_url: str,
    delta_path: str,
    token: str,
    *,
    batch_size: int = 5000,
) -> int:
    """Stream a large NDJSON export endpoint to Delta Lake.

    Parses zero bytes into Python strings — raw bytes fed to ``_parse``
    directly.  Spark write triggered every ``batch_size`` records.

    Parameters
    ----------
    spark:
        Active SparkSession.
    ndjson_url:
        URL returning an NDJSON response body.
    delta_path:
        Target Delta table path on DBFS.
    token:
        Bearer token.
    batch_size:
        Number of records per Spark write.
    """
    headers = {"Authorization": f"Bearer {token}"}
    batch: list[Any] = []
    total = 0

    with httpxr.Client(headers=headers) as client:
        with client.stream("GET", ndjson_url) as response:
            response.raise_for_status()
            for raw_line in iter_json_bytes(response):
                record = _parse(raw_line)
                batch.append(record)
                total += 1

                if len(batch) >= batch_size:
                    spark.createDataFrame(batch).write.format("delta").mode(
                        "append"
                    ).save(delta_path)
                    print(f"  Written {total} rows…")
                    batch = []

    if batch:
        spark.createDataFrame(batch).write.format("delta").mode("append").save(
            delta_path
        )

    print(f"✓ Streamed {total} records to {delta_path}")
    return total


# ---------------------------------------------------------------------------
# Salesforce example with paginate_to_records
# ---------------------------------------------------------------------------


def load_salesforce_to_delta(
    spark: Any,
    sf_instance_url: str,
    soql_query: str,
    delta_path: str,
    *,
    client_id: str,
    client_secret: str,
    batch_size: int = 2000,
) -> int:
    """Load Salesforce SOQL results into Delta Lake.

    Uses ``OAuth2Auth`` for token management and ``paginate_to_records``
    to follow ``nextRecordsUrl`` pagination automatically.
    """
    from httpxr.extensions import paginate_to_records

    auth = OAuth2Auth(
        token_url=f"{sf_instance_url}/services/oauth2/token",
        client_id=client_id,
        client_secret=client_secret,
    )

    rows: list[Any] = []
    total = 0

    with httpxr.Client(auth=auth) as client:
        for record in paginate_to_records(
            client,
            "GET",
            f"{sf_instance_url}/services/data/v58.0/query",
            params={"q": soql_query},
            records_key="records",
            next_func=lambda r: (
                sf_instance_url + r.json()["nextRecordsUrl"]
                if not r.json().get("done", True)
                else None
            ),
        ):
            rows.append(record)
            total += 1
            if len(rows) >= batch_size:
                spark.createDataFrame(rows).write.format("delta").mode("append").save(
                    delta_path
                )
                print(f"  Written {total} SF records…")
                rows = []

    if rows:
        spark.createDataFrame(rows).write.format("delta").mode("append").save(
            delta_path
        )

    print(f"✓ Loaded {total} Salesforce records to {delta_path}")
    return total


if __name__ == "__main__":
    # Demo: mock gather_raw_bytes without Spark
    def mock_handler(request: httpxr.Request) -> httpxr.Response:
        eid = request.url.path.split("/")[-1]
        return httpxr.Response(200, json={"id": eid, "name": f"Entity {eid}"})

    with httpxr.Client(transport=httpxr.MockTransport(mock_handler)) as c:
        reqs = [c.build_request("GET", f"http://api/entity/{i}") for i in range(20)]
        results = gather_raw_bytes(c, reqs, parser=json.loads, max_concurrency=10)
        print(f"Fetched {len(results)} entities concurrently:")
        for r in results[:3]:
            print(f"  {r}")
