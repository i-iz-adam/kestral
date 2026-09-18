---
name: Discord Server Builder
description: Complete guide to constructing Discord server structures, roles, categories, channels, and permission overwrites from prompt specifications.
triggers:
  - discord
  - discord server
  - build server
  - create role
  - channel permissions
---

# Discord Server Builder Guide

When asked to build, organize, or restructure a Discord server based on user prompts or specifications using a Discord connection (e.g. `The Magician`), follow this execution methodology.

## 1. Execution Order for Building a Server
To avoid broken references (e.g., trying to assign channel permissions to a role that does not exist yet), always build in this strict order:

1. **Query Existing Server State**:
   - Call `list_roles` to get current roles and their IDs (including `@everyone`).
   - Call `list_channels` to get current channels and category IDs.
2. **Create/Update Roles**:
   - Call `create_role` for each required role.
   - Save returned role IDs (snowflake IDs) for use in channel permission overwrites.
   - Supported parameters for `create_role`:
     - `name`: Role display name (e.g. `"Moderator"`)
     - `color`: Integer, hex string (e.g. `"#5865F2"`), or named color (`"BLURPLE"`, `"RED"`, `"GREEN"`, `"GOLD"`, `"PURPLE"`, `"CYAN"`, `"ORANGE"`)
     - `permissions`: Array of string permission names (e.g. `["VIEW_CHANNEL", "SEND_MESSAGES", "MANAGE_MESSAGES"]`), bitfield string (e.g. `"3072"`), or comma-separated names.
     - `hoist`: `true` to display role separately in member list.
     - `mentionable`: `true` to allow `@role` mentions.
3. **Create Categories**:
   - Call `create_category` (or `create_channel` with `type: 4`) for top-level category headers (e.g. `"COMMUNITY"`, `"STAFF ONLY"`, `"VOICE CHANNELS"`).
   - Pass `permission_overwrites` if category-level defaults apply.
   - Save returned Category IDs as `parent_id` for child channels.
4. **Create Channels**:
   - Call `create_channel` for each text, voice, or forum channel.
   - Parameters:
     - `name`: Channel slug (e.g. `"announcements"`, `"lounge"`, `"staff-chat"`)
     - `type`: `0` (text), `2` (voice), `4` (category), `5` (announcement), `15` (forum).
     - `parent_id`: Category ID from Step 3.
     - `topic`: Channel header description.
     - `permission_overwrites`: Array of overwrite rules.
5. **Set Specific Channel Permission Overwrites** (if not set during creation):
   - Call `set_channel_permissions`:
     - `channel_id`: Target channel ID.
     - `overwrite_id`: Role ID or User ID.
     - `type`: `0` for role, `1` for member.
     - `allow`: Permission names or bitfields to grant.
     - `deny`: Permission names or bitfields to restrict.

## 2. Common Permission Overwrite Patterns

### Private Staff / Admin Channel (Restricted to Staff Role & Admin)
To hide a channel from `@everyone` and allow only Staff/Admin:
- `@everyone` Role Overwrite: `deny: ["VIEW_CHANNEL"]`
- Staff Role Overwrite: `allow: ["VIEW_CHANNEL", "SEND_MESSAGES", "READ_MESSAGE_HISTORY", "ATTACH_FILES"]`

### Read-Only Announcement Channel
To allow members to read but only Staff to post:
- `@everyone` Role Overwrite: `allow: ["VIEW_CHANNEL", "READ_MESSAGE_HISTORY"]`, `deny: ["SEND_MESSAGES", "ADD_REACTIONS"]`
- Staff Role Overwrite: `allow: ["SEND_MESSAGES", "MANAGE_MESSAGES"]`

### Ticket / Private Support Channel
- `@everyone` Role Overwrite: `deny: ["VIEW_CHANNEL"]`
- Ticket Creator User Overwrite (`type: 1`): `allow: ["VIEW_CHANNEL", "SEND_MESSAGES", "ATTACH_FILES"]`
- Support Staff Role Overwrite (`type: 0`): `allow: ["VIEW_CHANNEL", "SEND_MESSAGES", "MANAGE_MESSAGES"]`

## 3. Supported Action Aliases & Actions
When invoking `integration_action`:
- Role actions: `create_role`, `edit_role`, `delete_role`, `list_roles`, `get_role`, `reorder_roles`
- Channel actions: `create_channel`, `create_category`, `edit_channel`, `delete_channel`, `get_channel`, `list_channels`, `reorder_channels`
- Permissions: `set_channel_permissions`, `delete_channel_permissions`
- Guild / Server: `get_guild`, `edit_guild`
- Emojis: `list_emojis`, `create_emoji`, `delete_emoji`
- Direct Messages: `send_dm` (opens DM with `user_id` and sends message), `create_dm` (opens/retrieves DM channel for `user_id`), `list_dms` (lists active bot DM channels)
- Messaging & Threads: `send_message` (supports `channel_id` or `user_id`), `edit_message`, `delete_message`, `get_messages`, `create_thread`, `list_threads`
