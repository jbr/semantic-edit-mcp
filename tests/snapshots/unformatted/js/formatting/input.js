// JavaScript example
import { useState, useEffect } from "react";
import axios from "axios";

function enhancedTest() {
  console.log(
    "Enhanced logging with timestamp:",
    new Date().toISOString(),
    "This function demonstrates advanced semantic editing capabilities!",
  );

  const metadata = {
    status: "enhanced",
    method: "advanced_semantic_edit",
    timestamp: Date.now(),
    version: "3.0",
    features: ["caching", "validation", "error-handling"],
  };

  return metadata;
}

class UserManager {
  constructor(apiKey) {
    this.apiKey = apiKey;
    this.users = new Map();
  }
  // Static method to create user manager with default config
  static createDefault() {
    return new UserManager("default-api-key");
  }

  async loadUser(id) {
    // Enhanced user loading with better error handling and caching
    console.log(`Loading user with ID: ${id}`);

    if (this.users.has(id)) {
      console.log(`User ${id} found in cache`);
      return this.users.get(id);
    }

    try {
      console.log(`Fetching user ${id} from API...`);
      const response = await fetchUserData(id);

      if (!response || !response.data) {
        throw new Error(`Invalid response for user ${id}`);
      }

      this.users.set(id, response.data);
      console.log(`Successfully cached user ${id}`);
      return response.data;
    } catch (error) {
      console.error(`Failed to load user ${id}:`, error.message);
      throw new Error(`User loading failed: ${error.message}`);
    }
  }

  clearCache() {
    this.users.clear();
    console.log("User cache cleared");
  }

  getUserCount() {
    return this.users.size;
  }

  async bulkLoadUsers(userIds) {
    const promises = userIds.map((id) => this.loadUser(id));
    try {
      const users = await Promise.all(promises);
      console.log(`Successfully loaded ${users.length} users`);
      return users;
    } catch (error) {
      console.error("Failed to bulk load users:", error);
      throw error;
    }
  }
  async refreshUser(id) {
    console.log(`Refreshing user data for ID: ${id}`);

    // Force refresh by removing from cache first
    this.users.delete(id);

    try {
      const freshUserData = await this.loadUser(id);
      console.log(`Successfully refreshed user ${id}`);
      return freshUserData;
    } catch (error) {
      console.error(`Failed to refresh user ${id}:`, error);
      throw error;
    }
  }
  getUserById(id) {
    if (this.users.has(id)) {
      return this.users.get(id);
    }
    return null;
  }
}
