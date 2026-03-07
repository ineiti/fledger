import type { State } from "../pkg/danu.js";
import { RealmID } from "../pkg/danu.js";

/**
 * Returns the RealmID strings from a State snapshot.
 */
export function getRealmIds(state: State): string[] {
  return state.realm_ids.map((id) => id.toString());
}

/**
 * Returns true if the given realm ID string is present in the state.
 */
export function hasRealm(state: State, realmId: string): boolean {
  return state.realm_ids.some((id) => id.toString() === realmId);
}
