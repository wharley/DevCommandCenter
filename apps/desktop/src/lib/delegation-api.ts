import { invoke } from "@tauri-apps/api/core";
import { DELEGATION_METHODS } from "@dcc/contracts";
import type {
	CancelDelegationInput,
	CancelDelegationOutput,
	CompleteDelegationInput,
	CompleteDelegationOutput,
	CreateDelegationInput,
	CreateDelegationOutput,
	FailDelegationInput,
	FailDelegationOutput,
	GetDelegationInput,
	GetDelegationOutput,
	ListDelegationsInput,
	ListDelegationsOutput,
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

export function failDelegation(input: FailDelegationInput) {
	return invoke<FailDelegationOutput>(DELEGATION_METHODS.failDelegation, {
		input,
	});
}
