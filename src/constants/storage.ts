// Central place for local-storage keys that carry logic (referenced in multiple places or with migration semantics).
// Note: persistedstate stores keep their own localStorage keys inline in each persist config (single use, self-explanatory).

// client-setting key for the session refresh token (persisted when "remember me" is on).
export const SESSION_KEY = "airport.session";
// Flag marking that preferences have been migrated from the old native store (localStorage).
export const PREFS_MIGRATED_KEY = "airport.prefs.migrated";
// client-setting key for remembering the main window size.
export const MAIN_WINDOW_KEY = "airport.window.main";
