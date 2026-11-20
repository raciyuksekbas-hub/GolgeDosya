/**
 * Karşılaştır worker'ı: hizalama ve görünüm modeli arayüz iş parçacığının
 * dışında hesaplanır (bkz. `comparisonJob.ts`, `comparisonCompute.ts`). Yalnız motoru içe aktarır;
 * pdfjs, mammoth ve React buraya girmez.
 */
import { answerComparison, type ComparisonReply, type ComparisonRequest } from "./comparisonCompute";

const scope = self as unknown as {
  onmessage: ((event: MessageEvent<ComparisonRequest>) => void) | null;
  postMessage(message: ComparisonReply): void;
};

scope.onmessage = (event) => {
  scope.postMessage(answerComparison(event.data));
};
