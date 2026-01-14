import Batteries.Tactic.OpenPrivate
import Lean.Elab.DefView
import Lean.Elab.Import
import Lean.Language.Lean
import Lean.Replay

open Lean Elab Language

def setup (stx : HeaderSyntax) : ProcessingT IO (Except Lean.HeaderProcessedSnapshot Lean.SetupImportsResult) := do
  pure <| .ok {
    mainModuleName := `Lean4OJ.main,
    isModule := stx.isModule,
    imports := stx.imports,
    opts := {},
    trustLevel := 0,
  }

def Lean.Name.isStd : Name → Bool
  | .str .anonymous "Aesop" => true
  | .str .anonymous "Archive" => true
  | .str .anonymous "Batteries" => true
  | .str .anonymous "Counterexamples" => true
  | .str .anonymous "ImportGraph" => true
  | .str .anonymous "Init" => true
  | .str .anonymous "Lake" => true
  | .str .anonymous "Lean" => true
  | .str .anonymous "LeanSearchClient" => true
  | .str .anonymous "Mathlib" => true
  | .str .anonymous "Plausible" => true
  | .str .anonymous "ProofWidgets" => true
  | .str .anonymous "Qq" => true
  | .str .anonymous "Std" => true
  | .str .anonymous "docs" => true
  | .str .anonymous "references" => true
  | .str .anonymous "Lean4OJ" => true
  | _ => false

inductive JudgeStatus
  | JudgerReceived
  | TypeChecking
  | AxiomChecking
  | Replaying
  | WrongAnswer
  | Accepted
  | JudgementFailed

def JudgeStatus.toByte : JudgeStatus → UInt8
  | .JudgerReceived  => 3
  | .TypeChecking    => 4
  | .AxiomChecking   => 5
  | .Replaying       => 6
  | .WrongAnswer     => 8
  | .Accepted        => 9
  | .JudgementFailed => 10

open private Lean.Kernel.Environment.mk Lean.Kernel.Environment.extensions Lean.Kernel.Environment.irBaseExts from Lean.Environment in
def reduceEnv (env : Environment) (names : Std.HashSet Name) : Environment :=
  let inner := env.toKernelEnv
  let consts := inner.constants.map₁.filter fun k _ => !names.contains k
  let consts' : ConstMap := Lean.SMap.fromHashMap consts
  let env' := Lean.Kernel.Environment.mk consts' inner.quotInit inner.diagnostics inner.const2ModIdx (Lean.Kernel.Environment.extensions inner) (Lean.Kernel.Environment.irBaseExts inner) inner.header
  Environment.ofKernelEnv env'

def kitsune (n : Nat) : ByteArray :=
  let b0 : UInt8 := UInt8.ofNat (n &&& 0xFF)
  let b1 : UInt8 := UInt8.ofNat ((n >>> 8) &&& 0xFF)
  let b2 : UInt8 := UInt8.ofNat ((n >>> 16) &&& 0xFF)
  let b3 : UInt8 := UInt8.ofNat ((n >>> 24) &&& 0xFF)
  .mk #[b0, b1, b2, b3]

def report (s : ByteArray) : IO Unit := do
  let stdout ← IO.getStdout
  stdout.write s
  stdout.flush

def handleMessage (tot : Nat) (msg : Message) : IO Nat := do
  let cnt := match msg.severity with
    | .error => 1
    | _ => 0
  let e ← msg.toString true
  report (
    .mk #[JudgeStatus.TypeChecking.toByte, 2] ++
      kitsune e.utf8ByteSize ++ e.bytes ++
      .mk #[0]
  )
  pure (tot + cnt)

def extractAnswer : Option ConstantInfo → Option String
  | some ci =>
    match ci.value? with
    | some (.lit (.strVal s)) => some s
    | _ => none
  | none => none

def main (args : List String) : IO Unit := do
  let fileName::allowedAxiomsList := args | return
  searchPathRef.set (← addSearchPathFromEnv [])
  let allowedAxioms := Std.HashSet.ofList (allowedAxiomsList.map String.toName)

  let contents : String ←
    try
      IO.FS.readFile fileName
    catch e =>
      let e := e.toString
      report (
        .mk #[JudgeStatus.JudgementFailed.toByte, 1] ++
          kitsune e.utf8ByteSize ++ e.bytes ++
          .mk #[0]
      )
      return
  let ctx := Parser.mkInputContext contents "main.lean"

  report (.mk #[JudgeStatus.TypeChecking.toByte, 1, 0, 0, 0, 0, 0])

  let snap : InitialSnapshot ←
    try
      Lean.process setup none ⟨ctx⟩
    catch e =>
      let e := e.toString
      report (
        .mk #[JudgeStatus.WrongAnswer.toByte, 1] ++
          kitsune e.utf8ByteSize ++ e.bytes ++
          .mk #[0]
      )
      return
  let snaps := Language.toSnapshotTree snap

  let errMonad : IO Nat := snaps.foldM (fun (tot : Nat) (snap : Snapshot) => snap.diagnostics.msgLog.unreported.foldlM handleMessage tot) 0
  let err : Nat ←
    try
      errMonad
    catch e =>
      let e := e.toString
      report (
        .mk #[JudgeStatus.WrongAnswer.toByte, 1] ++
          kitsune e.utf8ByteSize ++ e.bytes ++
          .mk #[0]
      )
      return
  if err > 0 then
    report (.mk #[JudgeStatus.WrongAnswer.toByte, 0, 0])
    return

  let some (cmdState : Command.State) := Language.Lean.waitForFinalCmdState? snap |
    let e := "Failed to obtain final command state"
    report (
      .mk #[JudgeStatus.JudgementFailed.toByte, 2] ++
        kitsune e.utf8ByteSize ++ e.bytes ++
        .mk #[0]
    )
    return

  let some answer := extractAnswer (cmdState.env.find? `Lean4OJ.answer_str) |
    let e := "Failed to extract answer"
    report (
      .mk #[JudgeStatus.WrongAnswer.toByte, 2] ++
        kitsune e.utf8ByteSize ++ e.bytes ++
        .mk #[0]
    )
    return

  report (
    .mk #[JudgeStatus.AxiomChecking.toByte, 0, 1] ++
      kitsune answer.utf8ByteSize ++ answer.bytes
  )

  let (_, state) := CollectAxioms.collect `Lean4OJ.answer cmdState.env {}
  for Axiom in state.axioms do
    if !(allowedAxioms.contains Axiom) then
      let e := s!"Disallowed axiom: {Axiom}."
      report (
        .mk #[JudgeStatus.WrongAnswer.toByte, 2] ++
          kitsune e.utf8ByteSize ++ e.bytes ++
          .mk #[0]
      )
      return

  report (.mk #[JudgeStatus.Replaying.toByte, 0, 0])

  let mut consts : Std.HashMap Name ConstantInfo := {}
  for mod in cmdState.env.header.modules, data in cmdState.env.header.moduleData, idx in (*...* : Std.Rii Nat) do
    if !mod.module.getRoot.isStd then
      for name in data.constNames, ci in data.constants do
        consts := consts.insert name ci
    else
      for name in data.constNames, ci in data.constants do
        let idx := cmdState.env.const2ModIdx.get? name
        let eff := idx.bind (fun i => cmdState.env.header.modules[i]?)
        let clean := match eff with
          | some e => e.module.getRoot.isStd
          | none => false
        if !clean then
          consts := consts.insert name ci

  let replayMonad := (reduceEnv cmdState.env <| Std.HashSet.ofList consts.keys).replay consts
  try
    replayMonad
  catch e =>
    let e := e.toString
    report (
      .mk #[JudgeStatus.WrongAnswer.toByte, 2] ++
        kitsune e.utf8ByteSize ++ e.bytes ++
        .mk #[0]
    )
    return

  report (.mk #[JudgeStatus.Accepted.toByte, 0, 0])
