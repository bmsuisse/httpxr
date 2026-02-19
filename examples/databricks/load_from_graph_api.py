"""
Databricks Example: Load data from Microsoft Graph API → Delta Lake.

This example shows how to use httpxr.extensions to:
  - Authenticate with Azure AD using OAuth2Auth (auto token refresh)
  - Paginate through Graph API results with paginate_to_records()
  - Write records to Delta Lake via PySpark

Requirements (Databricks cluster):
  pip install httpxr
"""

from __future__ import annotations

import os
from typing import Any

import httpxr
from httpxr.extensions import OAuth2Auth, paginate_to_records

# ---------------------------------------------------------------------------
# Configuration — set these as Databricks secrets
# ---------------------------------------------------------------------------

TENANT_ID = os.environ["AZURE_TENANT_ID"]
CLIENT_ID = os.environ["AZURE_CLIENT_ID"]
CLIENT_SECRET = os.environ["AZURE_CLIENT_SECRET"]
GRAPH_BASE = "https://graph.microsoft.com/v1.0"


def load_users_to_delta(spark: Any, delta_path: str) -> int:
    """Fetch all M365 users and write them to a Delta table.

    Memory usage is O(batch_size) — pages are processed one at a time.
    """
    auth = OAuth2Auth(
        token_url=f"https://login.microsoftonline.com/{TENANT_ID}/oauth2/v2.0/token",
        client_id=CLIENT_ID,
        client_secret=CLIENT_SECRET,
        scope="https://graph.microsoft.com/.default",
    )

    users: list[dict[str, Any]] = []
    batch_size = 500
    total = 0

    with httpxr.Client(auth=auth) as client:
        # paginate_to_records() yields one user dict at a time — O(1) memory
        for user in paginate_to_records(
            client,
            "GET",
            f"{GRAPH_BASE}/users",
            records_key="value",
            next_url="@odata.nextLink",
            params={"$select": "id,displayName,mail,userPrincipalName,department"},
        ):
            users.append(user)
            total += 1

            if len(users) >= batch_size:
                df = spark.createDataFrame(users)
                df.write.format("delta").mode("append").save(delta_path)
                print(f"  Written {total} users so far…")
                users = []

    # Flush remainder
    if users:
        df = spark.createDataFrame(users)
        df.write.format("delta").mode("append").save(delta_path)

    print(f"✓ Loaded {total} users to {delta_path}")
    return total


def load_teams_channels(spark: Any, delta_path: str) -> None:
    """Fetch all Teams and their channels, write to Delta."""
    auth = OAuth2Auth(
        token_url=f"https://login.microsoftonline.com/{TENANT_ID}/oauth2/v2.0/token",
        client_id=CLIENT_ID,
        client_secret=CLIENT_SECRET,
        scope="https://graph.microsoft.com/.default",
    )

    with httpxr.Client(auth=auth) as client:
        # First get all teams
        teams = list(
            paginate_to_records(
                client,
                "GET",
                f"{GRAPH_BASE}/groups",
                records_key="value",
                next_url="@odata.nextLink",
                params={"$filter": "resourceProvisioningOptions/Any(x:x eq 'Team')"},
            )
        )
        print(f"Found {len(teams)} teams")

        rows: list[dict[str, Any]] = []
        for team in teams:
            team_id = team["id"]
            for channel in paginate_to_records(
                client,
                "GET",
                f"{GRAPH_BASE}/teams/{team_id}/channels",
                records_key="value",
            ):
                rows.append(
                    {
                        "team_id": team_id,
                        "team_name": team.get("displayName", ""),
                        "channel_id": channel["id"],
                        "channel_name": channel["displayName"],
                    }
                )

    if rows:
        spark.createDataFrame(rows).write.format("delta").mode("overwrite").save(
            delta_path
        )
        print(f"✓ Loaded {len(rows)} channels to {delta_path}")


if __name__ == "__main__":
    # Local test without Spark — just print records
    auth = OAuth2Auth(
        token_url=f"https://login.microsoftonline.com/{TENANT_ID}/oauth2/v2.0/token",
        client_id=CLIENT_ID,
        client_secret=CLIENT_SECRET,
        scope="https://graph.microsoft.com/.default",
    )
    with httpxr.Client(auth=auth) as client:
        for i, user in enumerate(
            paginate_to_records(
                client,
                "GET",
                f"{GRAPH_BASE}/users",
                records_key="value",
                next_url="@odata.nextLink",
                params={"$top": "5"},
            )
        ):
            print(f"  {i + 1}. {user.get('displayName')} <{user.get('mail')}>")
            if i >= 4:
                break
