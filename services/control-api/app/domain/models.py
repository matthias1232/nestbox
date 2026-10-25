from pydantic import BaseModel, Field
class SandboxCreate(BaseModel):
    name: str = Field(min_length=1, max_length=80)
    runtime: str = "custom"
    template: str = "custom"
    tools: list[str] = Field(default_factory=list)
    policy_id: str | None = None
class ModelSandboxCreate(BaseModel):
    name: str
    profile: str
    hardware_target: str = "local-gpu"
class PolicyCreate(BaseModel):
    name: str
    allowed_destinations: list[str] = Field(default_factory=list)
    proxy_observation: bool = False
    tls_inspection: bool = False
    syslog_enabled: bool = False
class MountCreate(BaseModel):
    sandbox_id: str
    volume: str
    mode: str
class LifecycleAction(BaseModel):
    action: str
