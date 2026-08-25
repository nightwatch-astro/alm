// Copyright (C) 2024-2026 Sjors Robroek
// SPDX-License-Identifier: AGPL-3.0-only

import type {
  ContractError,
  ErrorResponseEnvelope,
  OkResponseEnvelope,
  OperationEvent,
  OperationHandle,
  OperationId,
  OperationName,
  RequestEnvelope,
} from "./generated/envelope";

export type TypedRequestEnvelope<TPayload = unknown> = Omit<RequestEnvelope, "payload"> & {
  payload: TPayload;
};

export type TypedOkResponseEnvelope<TPayload = unknown> = Omit<
  OkResponseEnvelope,
  "payload"
> & {
  payload: TPayload;
};

export type ResponseEnvelope<TPayload = unknown> =
  | TypedOkResponseEnvelope<TPayload>
  | ErrorResponseEnvelope;

export interface AlmCancellationSignal {
  readonly aborted: boolean;
  readonly reason?: unknown;
}

export interface ExecuteOperationOptions {
  requestId?: string;
  signal?: AlmCancellationSignal;
}

export interface SubscribeOperationOptions {
  signal?: AlmCancellationSignal;
  afterSequence?: number;
}

export interface AlmClient {
  execute<TRequest = unknown, TResponse = unknown>(
    operation: OperationName,
    request: TRequest,
    options?: ExecuteOperationOptions,
  ): Promise<TResponse>;

  subscribe(
    operationId: OperationId,
    options?: SubscribeOperationOptions,
  ): AsyncIterable<OperationEvent>;
}

export interface AlmTransport {
  send<TRequest = unknown, TResponse = unknown>(
    envelope: TypedRequestEnvelope<TRequest>,
    options?: ExecuteOperationOptions,
  ): Promise<ResponseEnvelope<TResponse>>;

  subscribe(
    operationId: OperationId,
    options?: SubscribeOperationOptions,
  ): AsyncIterable<OperationEvent>;
}

export interface AlmClientOptions {
  contractVersion?: "1.0.0";
  createRequestId?: () => string;
}

export class AlmContractError extends Error {
  public readonly contractError: ContractError;
  public readonly requestId: string;

  public constructor(requestId: string, contractError: ContractError) {
    super(contractError.message);
    this.name = "AlmContractError";
    this.requestId = requestId;
    this.contractError = contractError;
  }
}

export type { ContractError, OperationEvent, OperationHandle, OperationId, OperationName };
