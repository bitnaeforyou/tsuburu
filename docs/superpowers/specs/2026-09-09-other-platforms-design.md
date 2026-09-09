# macOS 밖의 텍스트 인식 설계

작성일: 2026-09-09
선행 스펙: `2026-09-05-tsuburu-design.md`, `2026-09-07-dialogue-search-design.md`

## 1. 왜 비어 있었나

대사 검색은 `Ocr` 트레이트 하나만 본다. 그 뒤에 macOS의 Vision만 있었고 다른
곳은 `None`이었다. 인식 모델을 같이 배포하지 않는다는 원칙 때문인데, 원칙은
맞지만 결론이 틀렸다. Windows에는 OS가 주는 인식기가 있고, 리눅스에는
배포판이 패키지로 주는 것이 있다.

## 2. AVIF라는 벽

먼저 확인한 사실: hitomi는 같은 경로에서 **AVIF만** 준다. 확장자를 webp나
jpg로 바꾸면 404다(실측). 그래서 인식기 앞에 AVIF를 풀 수단이 반드시 필요하다.

순수 러스트 디코더를 넣는 길은 재봤다. `rav1d`(dav1d의 러스트 포팅)는
기본 기능으로 어셈블리를 C 툴체인으로 빌드하고, 기능을 끄면 컴파일이 깨진다.
2.6 MB 단일 바이너리와 C 없는 크로스 빌드를 지키는 값으로는 비싸다.

그래서 **플랫폼이 이미 가진 이미지 스택을 쓴다.** macOS는 ImageIO, Windows는
WIC, 리눅스는 시스템 도구.

## 3. Windows

`Windows.Media.Ocr`이 인식하고 `Windows.Graphics.Imaging`이 푼다. 둘 다 OS에
들어 있다. 축소는 `BitmapTransform`이 맡아 디코드와 한 번에 끝난다.

- 언어는 `OcrEngine::TryCreateFromLanguage`에 BCP-47로 준다. 그 언어 팩이
  없으면 사용자 프로필 언어로 물러선다. 틀린 언어로 읽는 것이 침묵보다 낫다.
- 신뢰도를 주지 않는다. 파이프라인은 문턱값과만 비교하므로 1.0으로 채운다.
- AVIF는 스토어의 무료 **AV1 Video Extension**이 있어야 WIC가 푼다. 없으면
  디코드 실패로 그렇게 말한다.

macOS에서 `--target x86_64-pc-windows-msvc`로 타입 검사까지 한다. 링크와 실행은
CI의 windows 러너 몫이다.

## 4. 리눅스와 그 밖의 유닉스

`tesseract`가 읽고, `ffmpeg`/`magick`/`convert`/`avifdec` 중 있는 것이 푼다.
`import-meta`가 `sqlite3`를 부르는 것과 같은 방식이다. 둘 중 하나라도 없으면
`platform_note()`가 무엇을 깔아야 하는지 말하고, UI가 그대로 보여준다.

**입력은 파이프가 아니라 파일로 준다.** AVIF는 ISOBMFF라 디코더가 안에서
탐색하는데, 파이프로 주면 ffmpeg가 "partial file"로 실패한다(실측). PNG는
표준 출력으로 받는다.

TSV로 받아 단어 행(level 5)을 블록·문단·줄 번호로 다시 묶는다. 줄 상자는
단어 상자들의 합집합, 신뢰도는 평균이다.

**설정은 재서 골랐다.** 같은 한국어 페이지에서 Vision이 읽은 낱말 8개를
기준으로:

| 폭 | psm 6 | psm 11 |
|---|---|---|
| 1125 | 4/8 | **7/8** |
| 2037 | 2/8 | 6/8 |
| 2900 | 3/8 | 6/8 |

말풍선은 균일 블록이 아니므로 sparse text(11)가 맞고, 키우면 오히려 나빠진다.

tesseract는 한글을 음절 단위로 쪼개 내놓기도 한다("코 헤 이 군"). 매치 코드가
한글 사이 공백을 버리므로 검색에는 영향이 없다 — 라틴 문자 사이 공백만
경계로 남는다.

모듈은 macOS에서도 컴파일한다(Vision이 우선이라 쓰이지는 않는다). 파싱 로직이
개발 기계와 CI의 macOS 잡에서도 검사되게 하려는 것이다.

## 5. 검증

- 리눅스: 컨테이너(debian trixie, tesseract 5.5, ffmpeg 7.1)에서 실제 hitomi
  AVIF 페이지를 넣어 `cargo test -- --include-ignored` 14개 통과.
- Windows: `x86_64-pc-windows-msvc` 타입 검사 통과. 실행은 CI 러너.

## 6. 하지 않은 것

- 순수 러스트 AVIF 디코더. 3절 참조.
- 리눅스에서 인식 언어 팩 자동 설치. 무엇이 없는지 말해주는 데까지만 한다.
