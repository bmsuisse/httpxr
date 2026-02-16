import pytest
import anyio

@pytest.mark.anyio
async def test_ki():
    async def raiser():
        raise KeyboardInterrupt()
    
    with pytest.raises(KeyboardInterrupt):
        await raiser()
