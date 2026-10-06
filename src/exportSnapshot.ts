export async function exportAfterSave(save:Promise<unknown>|null,exportData:()=>Promise<string>):Promise<string> {
  await save;
  return exportData();
}
