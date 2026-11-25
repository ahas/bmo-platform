// The agency's client (docs/openapi.json): the types openapi-typescript generates from the document, and
// openapi-fetch's typed calls over them, each route by its path.

import createClient, { type Client } from "openapi-fetch";

import type { components, paths } from "./schema";

export type { components, operations, paths } from "./schema";

/** The fetch a client asks: one request, one answer. `globalThis.fetch` is one. */
export type AgencyFetch = (input: Request) => Promise<Response>;

type Schemas = components["schemas"];

export type Health = Schemas["Health"];
export type Event = Schemas["Event"];
export type EventPage = Schemas["EventPage"];
export type Supervisor = Schemas["Supervisor"];
export type Role = Schemas["Role"];
export type Repository = Schemas["Repository"];
export type Session = Schemas["Session"];
export type Turn = Schemas["Turn"];
export type Run = Schemas["Run"];
export type LogLine = Schemas["LogLine"];
export type Execution = Schemas["Execution"];
export type Slots = Schemas["Slots"];
export type RoleSlots = Schemas["RoleSlots"];
export type SlotRun = Schemas["SlotRun"];
export type SlotRequest = Schemas["SlotRequest"];
export type SlotAdmission = Schemas["SlotAdmission"];
export type SlotRequestMoved = Schemas["SlotRequestMoved"];
export type OwnerRelease = Schemas["OwnerRelease"];
export type SupervisorRoom = Schemas["SupervisorRoom"];
export type SupervisorResetPoint = Schemas["SupervisorResetPoint"];
export type RepositoryCheckout = Schemas["RepositoryCheckout"];
export type SamplingSetting = Schemas["SamplingSetting"];
export type RoleSessionContext = Schemas["RoleSessionContext"];
export type ProviderModel = Schemas["ProviderModel"];
export type ProviderQuota = Schemas["ProviderQuota"];
export type SupervisorRoad = Schemas["SupervisorRoad"];
export type WorkNode = Schemas["WorkNode"];
export type Reservation = Schemas["Reservation"];
export type Place = Schemas["Place"];
export type Provider = Schemas["Provider"];
export type Model = Schemas["Model"];
export type Assignment = Schemas["Assignment"];
export type JobItem = Schemas["JobItem"];
export type Job = Schemas["Job"];
export type BackgroundWork = Schemas["BackgroundWork"];
export type SessionWork = Schemas["SessionWork"];
export type SessionLoads = Schemas["SessionLoads"];
export type WireError = Schemas["Error"];

export type AgencyClient = Client<paths>;

/**
 * The agency at `baseUrl`, sending `token` as its bearer token. Each call is a route of the OpenAPI
 * document, typed by it: `client.GET("/sessions/{sessionId}", { params: { path: { sessionId } } })`.
 */
export function agencyClient(baseUrl: string, token: string, fetch?: AgencyFetch): AgencyClient {
  return createClient<paths>({
    baseUrl,
    headers: { Authorization: `Bearer ${token}` },
    ...(fetch ? { fetch } : {}),
  });
}
