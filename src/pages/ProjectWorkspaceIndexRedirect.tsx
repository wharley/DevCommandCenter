import React, { useMemo } from "react";
import { Navigate, useParams } from "react-router-dom";

export default function ProjectWorkspaceIndexRedirect() {
  const { id: projectId } = useParams<{ id: string }>();

  const target = useMemo(() => {
    if (!projectId) return "/";
    const saved = localStorage.getItem(`dcc:project:${projectId}:workspace`);
    const workspace = saved === "agents" ? "agents" : "pipeline";
    return `/project/${projectId}/${workspace}`;
  }, [projectId]);

  return <Navigate to={target} replace />;
}
