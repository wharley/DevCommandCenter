import { invoke } from "@tauri-apps/api/core";
import { DELEGATION_METHODS } from "@dcc/contracts";
import type {
	ApproveDelegationInput,
	ApproveDelegationOutput,
	CancelDelegationInput,
	CancelDelegationOutput,
	CompleteDelegationInput,
	CompleteDelegationOutput,
	CreateDelegationInput,
	CreateDelegationOutput,
	DelegationResultTurnInput,
	DelegationResultTurnOutput,
	FailDelegationInput,
	FailDelegationOutput,
	GetDelegationInput,
	GetDelegationOutput,
	ListDelegationsInput,
	ListDelegationsOutput,
	RunDelegationInput,
	RunDelegationOutput,
	StartDelegationInput,
	StartDelegationOutput,
} from "@dcc/contracts";

export function createDelegation(input: CreateDelegationInput) {
	return invoke<CreateDelegationOutput>(DELEGATION_METHODS.createDelegation, {
		input,
	});
}

export function listDelegations(input: ListDelegationsInput) {
	return invoke<ListDelegationsOutput>(DELEGATION_METHODS.listDelegations, {
		input,
	});
}

export function getDelegation(input: GetDelegationInput) {
	return invoke<GetDelegationOutput>(DELEGATION_METHODS.getDelegation, {
		input,
	});
}

export function cancelDelegation(input: CancelDelegationInput) {
	return invoke<CancelDelegationOutput>(DELEGATION_METHODS.cancelDelegation, {
		input,
	});
}

export function startDelegation(input: StartDelegationInput) {
	return invoke<StartDelegationOutput>(DELEGATION_METHODS.startDelegation, {
		input,
	});
}

export function completeDelegation(input: CompleteDelegationInput) {
	return invoke<CompleteDelegationOutput>(
		DELEGATION_METHODS.completeDelegation,
		{ input },
	);
}

export function approveDelegation(input: ApproveDelegationInput) {
	return invoke<ApproveDelegationOutput>(DELEGATION_METHODS.approveDelegation, {
		input,
	});
}

export function failDelegation(input: FailDelegationInput) {
	return invoke<FailDelegationOutput>(DELEGATION_METHODS.failDelegation, {
		input,
	});
}

/** Starts a delegation end to end in the backend; see `delegation_runtime.rs`. */
export function runDelegation(input: RunDelegationInput) {
	return invoke<RunDelegationOutput>(DELEGATION_METHODS.runDelegation, { input });
}

/** The deterministic `[DCC] …` hand-back text for a finished delegation. */
export function delegationResultTurn(input: DelegationResultTurnInput) {
	return invoke<DelegationResultTurnOutput>(DELEGATION_METHODS.delegationResultTurn, {
		input,
	});
}
