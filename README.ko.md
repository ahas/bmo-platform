[English](README.md) | **한국어** | [日本語](README.ja.md) | [简体中文](README.zh-CN.md)

# agency

에이전트를 공급하고 전달하며 제어하는 서비스입니다. 이 안에서는 두 축이 돌아갑니다. 하나는 방을 소유하고
사람과 대화하는 슈퍼바이저이고, 다른 하나는 작업을 실행하는 역할(role)입니다. 호출자가 어떤 경로에서 어떤
역할의 실행(run)을 요청하면, agency는 그 역할의 상한 안에서 실행을 들여보내고, 역할의 순서에서 쿼터가 남아
있는 프로바이더를 배정해 시작시킵니다. 그런 다음 턴을 전달하고, 에이전트가 말하는 내용을 읽고, 실행을
끝냅니다. herdr는 세션 계약 뒤에 놓인 백엔드 중 하나입니다.

## 문서

- `docs/api.md`: 경계 계약입니다. agency가 노출하거나 호출하는 모든 라우트와 이벤트를 다룹니다.
- `docs/agents.md`: 두 축인 슈퍼바이저와 역할, 실행, 그리고 모든 에이전트에 닿는 단 하나의 인터페이스를
  설명합니다.
- `docs/supervisors.md`: 슈퍼바이저 행이 담는 내용과, 하나의 러너가 모든 슈퍼바이저의 턴을 실어 나르는
  방식을 설명합니다.
- `docs/agent-roles.md`: 역할 행이 담는 내용, 정책이 시드하는 역할들, 그 번들과 세션을 설명합니다.
- `docs/repositories.md`: 저장소 행이 담는 내용, 그곳의 실행이 넘겨받는 것, 잡(job)과 고정된 모델을
  설명합니다.
- `docs/job-review.md`: 잡이 끝나기 전에 주문자가 잡의 보고를 단계별로 검토하는 방식을 설명합니다.
- `docs/provider-order.md`: 실행이 프로바이더와 모델을 받는 방식입니다. 순서, 쿼터, 폴오버, 대기를
  다룹니다.
- `docs/role-slots.md`: 역할의 상한 안에서 실행을 들여보내는 방식입니다. 대기열, 예약, 슈퍼바이저를 따르는
  역할, 상시 브랜치 체크아웃을 다룹니다.
- `docs/sessions.md`: 슈퍼바이저 세션과 역할 세션을 유지하고, 재사용하고, 접고(fold), 교체하는 방식을
  설명합니다.
- `docs/keepalive-schedule.md`: 슈퍼바이저의 프리픽스를 따뜻하게 유지하려고 핑을 보내는 시점을 설명합니다.
- `docs/PERSONAL-DATA.md`: 저장소가 들이지 않는 개인 데이터와, 이를 지키는 검사를 설명합니다.
- `docs/`의 나머지 문서: 실행 중인 프로그램에 말을 넣는 방법(`agent-control.md`), 실행 명령줄, 계정과 그
  폴백, 사용량 윈도, 등록된 모델과 모델 파일, 저장소의 체크아웃과 워크벤치, 작업 노드, 도구, 실행 기록을
  다룹니다.

## 폴더

| Folder | Holds |
|---|---|
| `crates/` | Rust 크레이트 |
| `deploy/` | 서비스 유닛 |
| `docs/` | 정본 문서 |
| `packages/` | TypeScript 패키지, 하나의 Bun 워크스페이스 |
| `screen-kit/` | 사람의 PC에서 개인 앱 화면을 빌드하는 도구: 콘솔이 쓰는 릴리스의 bmo-ui 번들 빌드 |
| `policy/` | agency가 따르는 고정값 |
| `prompts/` | agency가 에이전트 실행을 구성할 때 쓰는 프롬프트 |
| `skills/` | 저장소가 함께 싣는 워커 스킬 |
| `tests/` | 모든 테스트 |
| `tools/` | agency가 도구 호스트에 제공하는 도구 |

## 구성

| Path | Holds |
|---|---|
| `crates/agent-session` | 세션 계약: 시작, 턴 보내기, 상태 읽기, 중단, 접기, 종료, 목록 |
| `crates/herdr` | herdr의 소켓 클라이언트 |
| `crates/agent-host` | 페인(pane)에 놓인 실행들의 감독자 |
| `crates/pane-head` | 페인 안에 서 있는 프로세스 |
| `crates/harness-machine` | 하네스가 머신에서 읽고, 쓰고, 시작하는 것 |
| `crates/harness-providers` | 각 프로바이더가 머신에 대해 답하는 것 |
| `crates/node-link` | 머신 사이의 연결 |
| `crates/llm-gateway` | 등록된 모델들의 앞단 |
| `crates/policy` | `policy/`를 읽는 리더 |
| `crates/cache-siren` | 계정들의 사용량 윈도, 그 리더, 캐시 수명 판단 |
| `crates/client` | 크레이트 `bmo-agency-client`: agency API의 모든 본문과 멤버를 Rust 타입으로, 라우트마다 메서드 하나씩 |
| `crates/store` | agency의 테이블, 그 마이그레이션, 그 위의 구문 |
| `crates/tool-call` | 도구 호스트 호출 프로토콜의 agency 쪽 구현으로, agency의 도구들이 공유합니다 |
| `crates/server` | 서비스 본체: 세션, 자리, 라인, 역할 슬롯, 제어 라우트, 동사. 바이너리는 `agency-server`와 `bmo-agency` |
| `tools/` | agency가 도구 호스트에 제공하는 도구: 절차들과 규칙집 리더 |
| `packages/` | TypeScript 패키지, 하나의 Bun 워크스페이스 |
| `screen-kit/` | 사람의 PC에서 개인 앱 화면을 빌드하는 도구: 콘솔이 쓰는 릴리스의 bmo-ui 번들 빌드 |
| `policy/`, `prompts/` | agency가 따르는 고정값과 프롬프트 |
| `deploy/` | 서비스 유닛: 배포할 때마다 재시작되는 agency 서버, 에이전트 호스트, 모델 게이트웨이, 그리고 agency 배포가 유지하지 않는 herdr의 유닛 |
| `testbed.toml` | 테스트베드 선언: 저장소를 어떻게 테스트하고, 검사하고, 빌드하고, 배포하는지. 배포 대상은 함께 쓰는 데이터 루트 `~/.local/share/bmo/data` 옆에 있는 agency 자체의 배포 트리 `~/.local/share/bmo-agency`입니다 |
| `install.toml` | 설치 선언: 새 머신에 agency를 설치할 때 필요한 것으로, `bmo install`이 읽습니다 |
| `tests/` | 모든 테스트 |

## 명령

```sh
cargo check --workspace --all-targets
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
(cd packages && bun install && bunx tsc --noEmit)
(cd tests/packages && bun test)
```

## 라이선스

저장소 자체의 코드는 MIT 라이선스이며, 전문은 루트의 `LICENSE`에 있습니다. `cache-siren` 크레이트는
라이선스를 확인할 수 없었던 업스트림 프로젝트에서 파생되었기 때문에 라이선스 선언이 없으며, 저장소의 MIT
라이선스 적용 대상이 아닙니다.

## 버전

버전은 세트 전체의 것이며 semver 형식으로, 빌드할 때 git 태그에서 읽습니다. 워크스페이스 매니페스트의 버전은
움직이지 않는 자리표시 값입니다.
