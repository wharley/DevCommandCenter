import { useEffect, useState } from "react";
import type { PushRegistration, PushStatus } from "./registration";

export function usePushStatus(push: PushRegistration): PushStatus {
	const [status, setStatus] = useState(() => push.status());
	useEffect(() => {
		setStatus(push.status());
		return push.onStatus(setStatus);
	}, [push]);
	return status;
}

/** What Settings says about this phone's notifications. */
export function pushStatusText(status: PushStatus): string {
	switch (status.kind) {
		case "on":
			return "Ligadas. O computador avisa quando um agente precisar de você ou terminar.";
		case "pending":
			return "Ligadas. O computador fica sabendo assim que a conexão voltar.";
		case "off":
		case "unset":
			return "Desligadas neste aparelho.";
		case "denied":
			return status.canAskAgain
				? "O iOS/Android não deu permissão para notificações."
				: "Bloqueadas nos ajustes do sistema. Ative lá para receber avisos.";
		case "unavailable":
			return "Este build não tem o projectId do EAS, então a Expo não emite token de push.";
		case "unsupported":
			return "Notificações só existem no app instalado (não no navegador).";
		case "error":
			return `Não deu para avisar o computador: ${status.message}`;
	}
}
