/**
 * Setup v2 API client — typed accessors for the stateless
 * `/v2/setup/*` endpoints introduced in Slice4 P3 (T3.2).
 *
 * Maps onto `SpaceSetupFacade` on the backend; replaces the legacy
 * stateful `SetupFacade` HTTP surface that lived under `/setup/*`.
 */

import { daemonClient } from './client'
import { DaemonApiError } from './errors'

// ── DTOs (mirror uc-daemon-contract::api::dto::v2::setup) ──────────────────

export interface InitializeSpaceRequest {
  passphrase: string
  passphraseConfirm: string
  deviceName?: string
}

export interface InitializeSpaceResponse {
  spaceId: string
  selfDeviceId: string
  fingerprint: string
}

export interface IssueInvitationResponse {
  code: string
  expiresAtMs: number
}

export interface RedeemRequest {
  code: string
  passphrase: string
}

export interface RedeemResponse {
  sponsorDeviceId: string
  sponsorIdentityFingerprint: string
  spaceId: string
  selfDeviceId: string
  selfIdentityFingerprint: string
}

export interface CurrentInvitation {
  code: string
  expiresAtMs: number
}

export interface SetupStateResponse {
  hasCompleted: boolean
  currentInvitation: CurrentInvitation | null
  deviceName: string | null
}

// ── Typed errors (HTTP status → discriminated union) ───────────────────────
//
// Backend returns descriptive English messages in the body; we keep the raw
// message attached for diagnostics but classify by HTTP status so callers can
// branch declaratively without string matching.

export type InitializeSpaceErrorKind =
  | 'passphrase_mismatch' // 400
  | 'device_name_required' // 400
  | 'already_initialized' // 409
  | 'already_setup' // 409
  | 'service_unavailable' // 503
  | 'internal' // 500

export type RedeemInvitationErrorKind =
  | 'invitation_not_found' // 404
  | 'invitation_expired' // 404 (message contains "expired")
  | 'passphrase_mismatch' // 400 ("wrong passphrase")
  | 'device_name_required' // 400 (rare in v2: backend auto-fills)
  | 'sponsor_rejected' // 409
  | 'sponsor_declined' // 409
  | 'sponsor_unreachable' // 503
  | 'timeout' // 503
  | 'connection_lost' // 503
  | 'service_unavailable' // 503
  | 'internal' // 500

export type IssueInvitationErrorKind =
  | 'network_not_started' // 503
  | 'service_unavailable' // 503
  | 'internal' // 500

export type CancelInvitationErrorKind =
  | 'not_issued' // 409
  | 'service_unavailable' // 503
  | 'internal' // 500

export type ResetErrorKind =
  | 'service_unavailable' // 503
  | 'internal' // 500

export type QuerySetupStateErrorKind =
  | 'service_unavailable' // 503
  | 'internal' // 500

export class SetupV2Error<K extends string> extends Error {
  readonly kind: K
  readonly httpStatus?: number
  readonly raw: string

  constructor(kind: K, raw: string, httpStatus?: number) {
    super(`${kind}: ${raw}`)
    this.name = 'SetupV2Error'
    this.kind = kind
    this.httpStatus = httpStatus
    this.raw = raw
  }
}

/** Server error body shape from `uc-daemon::api::dto::error::ApiErrorResponse`. */
interface DaemonErrorBody {
  code?: string
  message?: string
}

function pickStatus(err: unknown): number | undefined {
  // `daemonClient.handleResponse` does not preserve the HTTP status separately,
  // but it leaves the original "<status> on <endpoint>" prefix in `err.message`
  // whenever the server body lacks a top-level `error` field — which is always
  // the case for the daemon (it uses `{ code, message }`).
  if (err instanceof DaemonApiError && err.message) {
    const m = /^(\d{3})\s+on\s+/.exec(err.message)
    if (m) return Number(m[1])
  }
  return undefined
}

function pickBody(err: unknown): DaemonErrorBody {
  if (err instanceof DaemonApiError && err.details && typeof err.details === 'object') {
    return err.details as DaemonErrorBody
  }
  return {}
}

function rawMessage(err: unknown): string {
  const body = pickBody(err)
  if (body.message) return body.message
  if (err instanceof Error) return err.message
  return String(err)
}

function classifyInitializeError(err: unknown): SetupV2Error<InitializeSpaceErrorKind> {
  const status = pickStatus(err)
  const raw = rawMessage(err)
  const lower = raw.toLowerCase()
  if (status === 400) {
    if (lower.includes('device name')) {
      return new SetupV2Error('device_name_required', raw, status)
    }
    return new SetupV2Error('passphrase_mismatch', raw, status)
  }
  if (status === 409) {
    if (lower.includes('completed')) {
      return new SetupV2Error('already_setup', raw, status)
    }
    return new SetupV2Error('already_initialized', raw, status)
  }
  if (status === 503) return new SetupV2Error('service_unavailable', raw, status)
  return new SetupV2Error('internal', raw, status)
}

function classifyRedeemError(err: unknown): SetupV2Error<RedeemInvitationErrorKind> {
  const status = pickStatus(err)
  const raw = rawMessage(err)
  const lower = raw.toLowerCase()
  if (status === 404) {
    if (lower.includes('expired')) return new SetupV2Error('invitation_expired', raw, status)
    return new SetupV2Error('invitation_not_found', raw, status)
  }
  if (status === 400) {
    if (lower.includes('device name')) return new SetupV2Error('device_name_required', raw, status)
    return new SetupV2Error('passphrase_mismatch', raw, status)
  }
  if (status === 409) {
    if (lower.includes('declined')) return new SetupV2Error('sponsor_declined', raw, status)
    return new SetupV2Error('sponsor_rejected', raw, status)
  }
  if (status === 503) {
    if (lower.includes('timed out') || lower.includes('timeout')) {
      return new SetupV2Error('timeout', raw, status)
    }
    if (lower.includes('connection lost')) return new SetupV2Error('connection_lost', raw, status)
    if (lower.includes('sponsor')) return new SetupV2Error('sponsor_unreachable', raw, status)
    return new SetupV2Error('service_unavailable', raw, status)
  }
  return new SetupV2Error('internal', raw, status)
}

function classifyIssueError(err: unknown): SetupV2Error<IssueInvitationErrorKind> {
  const status = pickStatus(err)
  const raw = rawMessage(err)
  const lower = raw.toLowerCase()
  if (status === 503) {
    if (lower.includes('not started')) return new SetupV2Error('network_not_started', raw, status)
    return new SetupV2Error('service_unavailable', raw, status)
  }
  return new SetupV2Error('internal', raw, status)
}

function classifyCancelError(err: unknown): SetupV2Error<CancelInvitationErrorKind> {
  const status = pickStatus(err)
  const raw = rawMessage(err)
  if (status === 409) return new SetupV2Error('not_issued', raw, status)
  if (status === 503) return new SetupV2Error('service_unavailable', raw, status)
  return new SetupV2Error('internal', raw, status)
}

function classifyResetError(err: unknown): SetupV2Error<ResetErrorKind> {
  const status = pickStatus(err)
  const raw = rawMessage(err)
  if (status === 503) return new SetupV2Error('service_unavailable', raw, status)
  return new SetupV2Error('internal', raw, status)
}

function classifyQueryError(err: unknown): SetupV2Error<QuerySetupStateErrorKind> {
  const status = pickStatus(err)
  const raw = rawMessage(err)
  if (status === 503) return new SetupV2Error('service_unavailable', raw, status)
  return new SetupV2Error('internal', raw, status)
}

// ── HTTP calls ──────────────────────────────────────────────────────────────

const ROUTE = {
  initialize: '/v2/setup/initialize',
  issueInvitation: '/v2/setup/issue-invitation',
  redeem: '/v2/setup/redeem',
  cancel: '/v2/setup/cancel',
  reset: '/v2/setup/reset',
  state: '/v2/setup/state',
} as const

export async function initializeSpace(
  body: InitializeSpaceRequest
): Promise<InitializeSpaceResponse> {
  try {
    return await daemonClient.request<InitializeSpaceResponse>(ROUTE.initialize, {
      method: 'POST',
      body,
    })
  } catch (err) {
    throw classifyInitializeError(err)
  }
}

export async function issuePairingInvitation(): Promise<IssueInvitationResponse> {
  try {
    return await daemonClient.request<IssueInvitationResponse>(ROUTE.issueInvitation, {
      method: 'POST',
    })
  } catch (err) {
    throw classifyIssueError(err)
  }
}

/**
 * Backend invitation codes are formatted as `XXXX-XXXX` (8 alphanumerics +
 * a hyphen separator) and the rendezvous server compares them as-is — no
 * normalization on the server side. The frontend OTP input strips the
 * hyphen so callers may hand us a bare 8-char code; rebuild the canonical
 * form here so all redeem paths behave identically.
 */
function normalizeInvitationCode(raw: string): string {
  const clean = raw.toUpperCase().replace(/[^A-Z0-9]/g, '')
  if (clean.length !== 8) return raw
  return `${clean.slice(0, 4)}-${clean.slice(4)}`
}

export async function redeemInvitation(body: RedeemRequest): Promise<RedeemResponse> {
  try {
    return await daemonClient.request<RedeemResponse>(ROUTE.redeem, {
      method: 'POST',
      body: { ...body, code: normalizeInvitationCode(body.code) },
    })
  } catch (err) {
    throw classifyRedeemError(err)
  }
}

export async function cancelInvitation(): Promise<void> {
  try {
    await daemonClient.request<void>(ROUTE.cancel, { method: 'POST' })
  } catch (err) {
    throw classifyCancelError(err)
  }
}

export async function resetSetup(): Promise<void> {
  try {
    await daemonClient.request<void>(ROUTE.reset, { method: 'POST' })
  } catch (err) {
    throw classifyResetError(err)
  }
}

export async function getSetupState(): Promise<SetupStateResponse> {
  try {
    return await daemonClient.request<SetupStateResponse>(ROUTE.state)
  } catch (err) {
    throw classifyQueryError(err)
  }
}
