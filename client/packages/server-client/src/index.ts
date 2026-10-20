// The web console's server API (docs/openapi.json): the types openapi-typescript generates from the document,
// and openapi-fetch's typed calls over them, each route by its path.

import createClient, { type Client } from "openapi-fetch";

import type { components, paths } from "./schema.ts";

export type { components, operations, paths } from "./schema.ts";

type Schemas = components["schemas"];

export type Health = Schemas["Health"];
export type ModelState = Schemas["ModelState"];
export type MoveAsk = Schemas["MoveAsk"];
export type Moved = Schemas["Moved"];
export type SenderUser = Schemas["SenderUser"];
export type UserView = Schemas["UserView"];
export type Senders = Schemas["Senders"];
export type CheckAnswer = Schemas["CheckAnswer"];
export type GrantRow = Schemas["GrantRow"];
export type Grants = Schemas["Grants"];
export type Granted = Schemas["Granted"];
export type Revoked = Schemas["Revoked"];
export type PhoneMade = Schemas["PhoneMade"];
export type PhoneHolder = Schemas["PhoneHolder"];
export type Holdings = Schemas["Holdings"];
export type Rendered = Schemas["Rendered"];
export type Session = Schemas["Session"];
export type WireError = Schemas["WireError"];
export type SessionError = Schemas["SessionError"];

export type AccountServiceClient = Client<paths>;

/**
 * The account service at `baseUrl`, sending `token` as its bearer token. Each call is a route of the OpenAPI
 * document, typed by it: `client.GET("/users/{user}", { params: { path: { user } } })`.
 */
export function accountServiceClient(
  baseUrl: string,
  token: string,
  fetcher?: typeof globalThis.fetch
): AccountServiceClient {
  return createClient<paths>({
    baseUrl,
    headers: { Authorization: `Bearer ${token}` },
    ...(fetcher === undefined ? {} : { fetch: fetcher })
  });
}
