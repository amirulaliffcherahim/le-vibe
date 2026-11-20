from __future__ import annotations

from collections.abc import Iterator
import pytest

from le_vibe.providers import catalog


@pytest.fixture(autouse=True)
def isolated_catalog(tmp_path, monkeypatch) -> Iterator[None]:
    """Keep tests off the user's real catalog cache, so only the seed catalogs load."""
    monkeypatch.setenv("LE_VIBE_CATALOG_CACHE", str(tmp_path / "models-cache.json"))
    catalog._DYNAMIC_MODELS.clear()
    catalog._PROVIDER_DEFAULT.clear()
    catalog._CACHE_LOADED = False
    yield
    catalog._DYNAMIC_MODELS.clear()
    catalog._PROVIDER_DEFAULT.clear()
    catalog._CACHE_LOADED = False
