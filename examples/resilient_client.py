"""
Example: RetryConfig and RateLimit — resilient API clients

RetryConfig provides automatic retries with exponential backoff and jitter.
RateLimit adds token-bucket request throttling.

Both can be set at the client level (applied to all requests) or
per-request via extensions.

These are httpxr extensions — not available in httpx.
"""

import httpxr


def retry_config_basics() -> None:
    """Create and inspect a RetryConfig."""
    # Default config
    default = httpxr.RetryConfig()
    print("Default RetryConfig:")
    print(f"  max_retries:     {default.max_retries}")
    print(f"  backoff_factor:  {default.backoff_factor}")
    print(f"  retry_on_status: {default.retry_on_status}")
    print(f"  jitter:          {default.jitter}")

    # Custom config
    custom = httpxr.RetryConfig(
        max_retries=5,
        backoff_factor=1.0,
        retry_on_status=[429, 500, 502, 503],
        jitter=False,
    )
    print("\nCustom RetryConfig:")
    print(f"  max_retries:     {custom.max_retries}")
    print(f"  should_retry(429): {custom.should_retry(429)}")
    print(f"  should_retry(200): {custom.should_retry(200)}")


def client_level_retry() -> None:
    """Set retry at the client level — applies to ALL requests."""
    retry = httpxr.RetryConfig(
        max_retries=3,
        backoff_factor=0.5,
        retry_on_status=[429, 500, 502, 503, 504],
    )

    with httpxr.Client(retry=retry) as client:
        # All requests automatically retry on configured status codes
        r = client.get("https://httpbin.org/get")
        print(f"Client-level retry: {r.status_code}")
        print(f"  client.retry.max_retries = {client.retry.max_retries}")


def per_request_retry() -> None:
    """Override retry config per-request via extensions."""
    retry = httpxr.RetryConfig(max_retries=5)

    with httpxr.Client() as client:
        # Only this request gets retry
        r = client.get(
            "https://httpbin.org/get",
            extensions={"retry": retry},
        )
        print("Per-request retry:", r.status_code)


def rate_limit_basics() -> None:
    """Create and inspect a RateLimit."""
    rl = httpxr.RateLimit(
        requests_per_second=5.0,
        burst=10,
    )
    print(f"\nRateLimit:")
    print(f"  requests_per_second: {rl.requests_per_second}")
    print(f"  burst:               {rl.burst}")
    print(f"  wait_time():         {rl.wait_time():.4f}s")


def resilient_client() -> None:
    """Combine retry + rate limit for a production-ready client."""
    client = httpxr.Client(
        retry=httpxr.RetryConfig(
            max_retries=3,
            backoff_factor=0.5,
        ),
        rate_limit=httpxr.RateLimit(
            requests_per_second=10.0,
            burst=20,
        ),
        base_url="https://httpbin.org",
    )

    print("\nResilient client:")
    print(f"  retry:      {client.retry.max_retries} retries")
    print(f"  rate_limit: {client.rate_limit.requests_per_second} req/s")

    r = client.get("/get")
    print(f"  GET /get: {r.status_code}")
    client.close()


def dynamic_config() -> None:
    """Change retry/rate_limit after client creation."""
    with httpxr.Client() as client:
        # Start without retry
        assert client.retry is None

        # Add retry dynamically
        client.retry = httpxr.RetryConfig(max_retries=3)
        print(f"\nDynamic: retry set to {client.retry.max_retries} retries")

        # Add rate limit dynamically
        client.rate_limit = httpxr.RateLimit(requests_per_second=50.0)
        print(
            f"Dynamic: rate_limit set to"
            f" {client.rate_limit.requests_per_second} req/s"
        )

        # Remove them
        client.retry = None
        client.rate_limit = None
        print("Dynamic: both cleared (None)")


if __name__ == "__main__":
    print("=== RetryConfig Basics ===")
    retry_config_basics()

    print("\n=== Client-Level Retry ===")
    client_level_retry()

    print("\n=== Per-Request Retry ===")
    per_request_retry()

    print("\n=== RateLimit Basics ===")
    rate_limit_basics()

    print("\n=== Resilient Client ===")
    resilient_client()

    print("\n=== Dynamic Config ===")
    dynamic_config()
