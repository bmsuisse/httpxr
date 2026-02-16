import pytest
import httpr

@pytest.mark.anyio
async def test_ki_basic():
    try:
        raise KeyboardInterrupt("Simulated")
    except KeyboardInterrupt:
        pass # Caught

@pytest.mark.anyio
async def test_ki_async_propagation():
    async def raiser():
        raise KeyboardInterrupt("Simulated Async")
    
    with pytest.raises(KeyboardInterrupt):
        await raiser()
