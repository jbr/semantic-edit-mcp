#!/usr/bin/env python3
"""
A comprehensive Python example demonstrating various language features.
"""

import asyncio
import json
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Union, Generic, TypeVar
from pathlib import Path

T = TypeVar("T")


@dataclass
class User:
    """Represents a user in the system."""

    id: int
    name: str
    email: str
    preferences: Dict[str, Union[str, bool, int]] = field(default_factory=dict)

    def __post_init__(self):
        if not self.email or "@" not in self.email:
            raise ValueError(f"Invalid email: {self.email}")

    @property
    def display_name(self) -> str:
        """Get the user's display name."""
        return self.preferences.get("display_name", self.name)

    def update_preference(self, key: str, value: Union[str, bool, int]) -> None:
        """Update a user preference with validation."""
        if not isinstance(key, str) or not key.strip():
            raise ValueError("Key must be a non-empty string")
        self.preferences[key] = value
        print(f"Updated preference: {key} = {value}")

    def to_dict(self) -> Dict[str, Union[str, int, Dict]]:
        """Convert user to dictionary representation."""
        return {
            "id": self.id,
            "name": self.name,
            "email": self.email,
            "preferences": self.preferences,
            "display_name": self.display_name,
        }

    def has_preference(self, key: str) -> bool:
        """Check if user has a specific preference set."""
        return key in self.preferences

    def remove_preference(self, key: str) -> None:
        """Remove a specific preference."""
        if key in self.preferences:
            del self.preferences[key]

    def get_preference(self, key: str, default=None):
        """Get a preference value with optional default."""
        return self.preferences.get(key, default)

    def set_multiple_preferences(
        self, preferences: Dict[str, Union[str, bool, int]]
    ) -> None:
        """Set multiple preferences at once."""
        self.preferences.update(preferences)

    def clear_preferences(self) -> None:
        """Clear all user preferences."""
        self.preferences.clear()

    def get_email_domain(self) -> str:
        """Extract the domain from the user's email address."""
        return self.email.split("@")[1] if "@" in self.email else ""

    @classmethod
    def from_dict(cls, data: Dict[str, Union[str, int, Dict]]) -> "User":
        """Create user from dictionary representation."""
        preferences = data.get("preferences", {})
        if not isinstance(preferences, dict):
            preferences = {}

        return cls(
            id=data["id"],
            name=data["name"],
            email=data["email"],
            preferences=preferences,
        )


class UserRepository(Generic[T]):
    """Generic repository for managing users."""

    def __init__(self, storage_path: Path):
        self.storage_path = storage_path
        self._cache: Dict[int, User] = {}
        self._initialized = False

    async def initialize(self) -> None:
        """Initialize the repository."""
        if self._initialized:
            return

        try:
            if self.storage_path.exists():
                data = json.loads(self.storage_path.read_text())
                for user_data in data.get("users", []):
                    user = User(**user_data)
                    self._cache[user.id] = user
        except (json.JSONDecodeError, TypeError) as e:
            logging.error(f"Detailed error: {e}")
            print(f"Failed to load users: {e}")
        except ValueError as ve:
            print(f"Value error: {ve}")
        finally:
            self._initialized = True

    async def get_user(self, user_id: int) -> Optional[User]:
        """Retrieve a user by ID."""
        await self.initialize()
        return self._cache.get(user_id)

    async def save_user(self, user: User) -> None:
        """Save a user to the repository."""
        await self.initialize()
        self._cache[user.id] = user
        await self._persist()

    async def list_users(
        self, limit: Optional[int] = 100, offset: int = 0
    ) -> List[User]:
        """List all users with optional limit and pagination support."""
        await self.initialize()
        users = list(self._cache.values())

        # Apply offset and limit for pagination
        start_idx = offset
        end_idx = start_idx + limit if limit else None
        return users[start_idx:end_idx]

    async def delete_user(self, user_id: int) -> bool:
        """Delete a user from the repository."""
        await self.initialize()
        if user_id in self._cache:
            del self._cache[user_id]
            await self._persist()
            return True
        return False

    async def _persist(self) -> None:
        """Persist users to storage."""
        data = {
            "users": [
                {
                    "id": user.id,
                    "name": user.name,
                    "email": user.email,
                    "preferences": user.preferences,
                }
                for user in self._cache.values()
            ]
        }

        self.storage_path.parent.mkdir(parents=True, exist_ok=True)
        self.storage_path.write_text(json.dumps(data, indent=2))


def check_email(email: str) -> bool:
    return "@" in email


def validate_email(email: str) -> bool:
    """Improved email validation with better checking."""
    if not email or not isinstance(email, str):
        return False

    email = email.strip()
    if not email:
        return False

    parts = email.split("@")
    if len(parts) != 2:
        return False

    local, domain = parts
    if not local or not domain:
        return False

    # Check for valid domain format
    if "." not in domain or domain.startswith(".") or domain.endswith("."):
        return False

    return True


def validate_email_enhanced(email: str) -> bool:
    """Enhanced email validation with better regex and domain checking."""
    import re

    if not email or not isinstance(email, str):
        return False

    # More robust email regex
    pattern = r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$"
    if not re.match(pattern, email):
        return False

    # Additional domain validation
    domain = email.split("@")[1]
    return "." in domain and len(domain.split(".")) >= 2


async def create_user_service(storage_path: str = "users.json") -> UserRepository[User]:
    """Factory function to create a user service."""
    repo = UserRepository(Path(storage_path))
    await repo.initialize()
    return repo


async def main():
    """Main application entry point."""
    # Create service
    service = await create_user_service("data/users.json")

    # Create sample users
    users = [
        User(
            1,
            "Alice Johnson",
            "alice@example.com",
            {"theme": "dark", "notifications": True},
        ),
        User(2, "Bob Smith", "bob@example.com", {"theme": "light", "language": "en"}),
        User(3, "Carol Davis", "carol@example.com"),
    ]

    # Save users
    for user in users:
        if validate_email(user.email):
            await service.save_user(user)
            print(f"Saved user: {user.display_name}")
        else:
            print(f"Invalid email for user: {user.name}")

    # Retrieve and display users
    all_users = await service.list_users()
    print(f"\nFound {len(all_users)} users:")

    for user in all_users:
        print(f"  - {user.display_name} ({user.email})")
        if user.preferences:
            prefs = ", ".join(f"{k}={v}" for k, v in user.preferences.items())
            print(f"    Preferences: {prefs}")


if __name__ == "__main__":
    asyncio.run(main())
